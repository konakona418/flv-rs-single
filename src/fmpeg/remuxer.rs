use std::any::Any;
use crate::core::Core;
use crate::exchange::PackedContentToCore::Data;
use crate::exchange::{Destination, MseDecoderConfig, Packed, PackedContent, PackedContentToCore, PackedContentToRemuxer, RemuxedData};
use crate::flv::header::{FlvHeader, TagHeader};
use crate::flv::meta::RawMetaData;
use crate::flv::tag::{Tag, TagType};
use crate::fmpeg::encoder::{Encoder, DEFAULT_AUDIO_TRACK_ID, DEFAULT_VIDEO_TRACK_ID};
use crate::fmpeg::mp4head::ISerializable;
use crate::fmpeg::parser::{parse_aac_timescale, parse_avc_timescale, parse_mp3_timescale, parse_timescale, parse_timescale_signed, AudioParseResult, Avc1ParseResult, KeyframeType, Parser, VideoParseResult};
use crate::fmpeg::remux_context::{RemuxContext, SampleContextBuilder, TrackContext, TrackType, VideoSequenceBufferEntry};
use std::cmp::PartialEq;
use std::collections::VecDeque;
use std::thread::JoinHandle;

pub struct Remuxer {
    remuxing: bool,
    pack_buffer: VecDeque<Packed>,
    pub core: Core,

    video_sequence_buffer: VecDeque<VideoSequenceBufferEntry>,
    video_dts_adjust: Option<u32>,

    audio_sequence_buffer: VecDeque<VideoSequenceBufferEntry>,
    audio_dts_adjust: Option<u32>,

    frame_count: u32,

    tags: VecDeque<Tag>,
    metadata: Option<RawMetaData>,
    flv_header: Option<FlvHeader>,

    ctx: RemuxContext,

    audio_track: TrackContext,
    video_track: TrackContext,

    _temp: Option<Vec<u8>>
}

impl PartialEq for KeyframeType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (KeyframeType::Keyframe, KeyframeType::Keyframe) => true,
            (KeyframeType::Interframe, KeyframeType::Interframe) => true,
            _ => false
        }
    }
}

impl Remuxer {
    pub fn new() -> Self {
        Self {
            remuxing: false,
            pack_buffer: VecDeque::new(),
            core: Core::new(),

            video_sequence_buffer: VecDeque::new(),
            video_dts_adjust: None,

            audio_sequence_buffer: VecDeque::new(),
            audio_dts_adjust: None,

            frame_count: 0,

            tags: VecDeque::new(),
            metadata: None,
            flv_header: None,
            ctx: RemuxContext::new(),


            audio_track: TrackContext::new(DEFAULT_AUDIO_TRACK_ID, TrackType::Audio),
            video_track: TrackContext::new(DEFAULT_VIDEO_TRACK_ID, TrackType::Video),

            _temp: None
        }
    }

    #[inline]
    fn set_remuxing(&mut self, flag: bool) {
        self.remuxing = flag;
    }

    fn send(&mut self, pack: Packed) -> Result<(), Box<dyn std::error::Error>> {
        self.core.push_pack(pack);
        Ok(())
    }

    fn send_mpeg4_header(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut header = Encoder::encode_ftyp(&self.ctx).serialize();
        header.append(&mut Encoder::encode_moov(&self.ctx).serialize());
        self.ctx.set_header_sent(true);

        self.send(
            Packed {
                packed_routing: Destination::Core,
                packed_content: PackedContent::ToCore(Data(RemuxedData::Header(header))),
            }
        )
    }

    fn send_raw_data(&mut self, data: RemuxedData) -> Result<(), Box<dyn std::error::Error>> {
        self.send(
            Packed {
                packed_routing: Destination::Core,
                packed_content: PackedContent::ToCore(Data(data)),
            }
        )
    }

    fn remux(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.ctx.is_configured() && !self.ctx.is_header_sent() {
            self.send_mpeg4_header()?;
            if let Some(tmp) = self._temp.take() {
                self.send_raw_data(RemuxedData::Audio(tmp))?;
            }
        }

        while let Some(tag) = self.tags.pop_front() {
            match tag.tag_type {
                TagType::Audio => {
                    let parsed: AudioParseResult = Parser::parse_audio(&tag)?;
                    if self.ctx.is_configured() {
                        if !self.ctx.is_header_sent() {
                            self.send_mpeg4_header()?;
                            if let Some(tmp) = self._temp.take() {
                                self.send_raw_data(RemuxedData::Audio(tmp))?;
                            }
                        }
                        match parsed {
                            AudioParseResult::AacRaw(raw) => {
                                if !self.audio_sequence_buffer.is_empty() {
                                    let mut prev_sample = self.audio_sequence_buffer.iter_mut().last().unwrap();

                                    let prev_dts = prev_sample.sample_ctx.decode_time;
                                    let current_dts = parse_timescale(tag.timestamp) - self.audio_dts_adjust.unwrap_or(0);

                                    let prev_duration_corrected = current_dts - prev_dts;

                                    prev_sample.sample_ctx.sample_duration = prev_duration_corrected;

                                    let mut sample_ctx = SampleContextBuilder::new()
                                        .set_decode_time(parse_timescale(tag.timestamp))
                                        .set_sample_size(raw.len() as u32)
                                        .set_sample_duration(parse_aac_timescale(self.ctx.audio_sample_rate))
                                        .set_composition_time_offset(0)
                                        .build();

                                    self.audio_sequence_buffer.push_back(VideoSequenceBufferEntry::new(Vec::from(raw), sample_ctx));

                                    let front = self.audio_sequence_buffer.pop_front();
                                    if let Some(mut front) = front {
                                        let mut data = Encoder::encode_moof(&mut self.ctx, &mut self.audio_track, &mut front.sample_ctx).serialize();
                                        data.append(&mut Encoder::encode_mdat(front.payload).serialize());
                                        self.send_raw_data(RemuxedData::Audio(data))?;
                                    }
                                } else {
                                    let sample_ctx = SampleContextBuilder::new()
                                        .set_decode_time(parse_timescale(tag.timestamp))
                                        .set_sample_size(raw.len() as u32)
                                        .set_sample_duration(parse_aac_timescale(self.ctx.audio_sample_rate))
                                        .set_composition_time_offset(0)
                                        .build();

                                    self.audio_sequence_buffer.push_back(VideoSequenceBufferEntry::new(Vec::from(raw), sample_ctx));
                                }
                            }
                            AudioParseResult::Mp3(parsed) => {
                                let mut sample_ctx = SampleContextBuilder::new()
                                    .set_decode_time(parse_timescale(tag.timestamp))
                                    .set_sample_size(parsed.body.len() as u32)
                                    .set_sample_duration(parse_mp3_timescale(parsed.sample_rate, parsed.version))
                                    .set_composition_time_offset(0)
                                    .build();

                                let mut data = Encoder::encode_moof(&mut self.ctx, &mut self.audio_track, &mut sample_ctx).serialize();
                                data.append(&mut Encoder::encode_mdat(parsed.body).serialize());
                                self.send_raw_data(RemuxedData::Audio(data))?;
                            }
                            _ => {
                                panic!("[Remuxer] Aac format header not set!")
                            }
                        }
                    } else {
                        let audio_codec_conf =  self.ctx.configure_audio_metadata(&parsed);

                        if let AudioParseResult::Mp3(parsed) = parsed {
                            let mut sample_ctx = SampleContextBuilder::new()
                                .set_decode_time(parse_timescale(tag.timestamp))
                                .set_sample_size(parsed.body.len() as u32)
                                .set_sample_duration(parse_mp3_timescale(parsed.sample_rate, parsed.version))
                                .set_composition_time_offset(0)
                                .build();

                            let mut data = Encoder::encode_moof(&mut self.ctx, &mut self.audio_track, &mut sample_ctx).serialize();
                            data.append(&mut Encoder::encode_mdat(parsed.body).serialize());
                            self._temp = Some(data);
                        }

                        if let Some(conf) = audio_codec_conf {
                            self.send( Packed {
                                packed_routing: Destination::Core,
                                packed_content: PackedContent::ToCore(
                                    PackedContentToCore::DecoderConfig(
                                        MseDecoderConfig::AudioCodec(conf)
                                    )
                                )
                            })?;
                        }
                    }
                }
                TagType::Video => {
                    let parsed: VideoParseResult = Parser::parse_video(&tag)?;
                    if self.ctx.is_configured() {
                        if !self.ctx.is_header_sent() {
                            self.send_mpeg4_header()?;
                            if let Some(tmp) = self._temp.take() {
                                self.send_raw_data(RemuxedData::Video(tmp))?;
                            }
                        }
                        if let VideoParseResult::Avc1(parsed) = parsed {
                            match parsed {
                                Avc1ParseResult::AvcNalu(data) => {
                                    /*if data.keyframe_type == KeyframeType::Keyframe {
                                        if self.video_sequence_buffer.is_empty() {
                                            // if this frame is a keyframe, and there's no existing keyframe,
                                            // then buffer it.
                                            println!("No keyframe found, buffering keyframe");
                                            self.frame_count += 1;
                                            let sample_ctx = SampleContextBuilder::new()
                                                .set_decode_time(parse_timescale(tag.timestamp))
                                                .set_sample_size(data.payload.len() as u32)
                                                .set_sample_duration(parse_avc_timescale(self.ctx.fps as f32))
                                                .set_composition_time_offset(0)
                                                .set_has_redundancy(false)
                                                .set_is_leading(self.video_track.sequence_number == 1)
                                                .set_is_keyframe(data.keyframe_type == KeyframeType::Keyframe)
                                                .set_is_non_sync(data.keyframe_type == KeyframeType::Interframe)
                                                .build();
                                            self.video_sequence_buffer.push_back(VideoSequenceBufferEntry::new(Vec::from(data.payload), sample_ctx));
                                        } else {
                                            // if this frame is a keyframe, and there's existing keyframe,
                                            // then drain the buffer, and push this frame to the buffer.
                                            println!("drain buffer");
                                            println!("existing keyframe: {:?}", self.video_sequence_buffer.len());
                                            self.frame_count += self.video_sequence_buffer.len() as u32;
                                            let mut entries = self.video_sequence_buffer.drain(..).collect::<Vec<_>>();
                                            let mut contexts = Vec::new();
                                            let mut data_mdat = Vec::new();
                                            for entry in entries {
                                                contexts.push(entry.sample_ctx);
                                                data_mdat.push(entry.payload);
                                            }
                                            let mut send_data = Encoder::encode_moof_merged(&mut self.ctx, &mut self.video_track, &mut contexts).serialize();
                                            send_data.append(&mut Encoder::encode_mdat_merged(data_mdat).serialize());
                                            self.send_raw_data(RemuxedData::Video(send_data))?;

                                            let sample_ctx = SampleContextBuilder::new()
                                                .set_decode_time(parse_timescale(tag.timestamp))
                                                .set_sample_size(data.payload.len() as u32)
                                                .set_sample_duration(parse_avc_timescale(self.ctx.fps as f32))
                                                .set_composition_time_offset(0)
                                                .set_has_redundancy(false)
                                                .set_is_leading(self.video_track.sequence_number == 1)
                                                .set_is_keyframe(data.keyframe_type == KeyframeType::Keyframe)
                                                .set_is_non_sync(data.keyframe_type == KeyframeType::Interframe)
                                                .build();
                                            self.video_sequence_buffer.push_back(VideoSequenceBufferEntry::new(Vec::from(data.payload), sample_ctx));
                                        }
                                    } else {
                                        if self.video_sequence_buffer.is_empty() {
                                            // if this frame is not a keyframe, and there's no existing keyframe,
                                            // then directly send it.
                                            self.frame_count += 1;
                                            println!("No keyframe found, sending interframe");
                                            let mut sample_ctx = SampleContextBuilder::new()
                                                .set_decode_time(parse_timescale(tag.timestamp))
                                                .set_sample_size(data.payload.len() as u32)
                                                .set_sample_duration(parse_avc_timescale(self.ctx.fps as f32))
                                                .set_composition_time_offset(0)
                                                .set_has_redundancy(false)
                                                .set_is_leading(self.video_track.sequence_number == 1)
                                                .set_is_keyframe(data.keyframe_type == KeyframeType::Keyframe)
                                                .set_is_non_sync(data.keyframe_type == KeyframeType::Interframe)
                                                .build();

                                            let mut send_data = Encoder::encode_moof(&mut self.ctx, &mut self.video_track, &mut sample_ctx).serialize();
                                            send_data.append(&mut Encoder::encode_mdat(Vec::from(data.payload)).serialize());
                                            self.send_raw_data(RemuxedData::Video(send_data))?;
                                        } else {
                                            // if this frame is not a keyframe, and there's an existing keyframe,
                                            // then push this frame to the end of the buffer.
                                            println!("Push interframe to buffer");
                                            let sample_ctx = SampleContextBuilder::new()
                                                .set_decode_time(parse_timescale(tag.timestamp))
                                                .set_sample_size(data.payload.len() as u32)
                                                .set_sample_duration(parse_avc_timescale(self.ctx.fps as f32))
                                                .set_composition_time_offset(0)
                                                .set_has_redundancy(false)
                                                .set_is_leading(self.video_track.sequence_number == 1)
                                                .set_is_keyframe(data.keyframe_type == KeyframeType::Keyframe)
                                                .set_is_non_sync(data.keyframe_type == KeyframeType::Interframe)
                                                .build();
                                            self.video_sequence_buffer.push_back(VideoSequenceBufferEntry::new(Vec::from(data.payload), sample_ctx))
                                        }
                                    }*/
                                    let cts = if let TagHeader::Video(ref header) = tag.tag_header {
                                        header.composition_time_offset.unwrap_or(0)
                                    } else {
                                        0
                                    };

                                    if !self.video_sequence_buffer.is_empty() {
                                        let mut prev_sample = self.video_sequence_buffer.iter_mut().last().unwrap();

                                        let prev_dts = prev_sample.sample_ctx.decode_time;
                                        let current_dts = parse_timescale(tag.timestamp);

                                        let prev_duration_correction = current_dts - prev_dts;
                                        prev_sample.sample_ctx.sample_duration = prev_duration_correction;

                                        let dts_correction = parse_timescale(tag.timestamp) - self.video_dts_adjust.unwrap_or(0);

                                        let sample_ctx = SampleContextBuilder::new()
                                            .set_decode_time(dts_correction)
                                            .set_sample_size(data.payload.len() as u32)
                                            .set_sample_duration(parse_avc_timescale(self.ctx.fps as f32))
                                            .set_composition_time_offset(parse_timescale_signed(cts))
                                            .set_has_redundancy(false)
                                            .set_is_leading(self.video_track.sequence_number == 1)
                                            .set_is_keyframe(data.keyframe_type == KeyframeType::Keyframe)
                                            .set_is_non_sync(data.keyframe_type == KeyframeType::Interframe)
                                            .build();

                                        self.video_sequence_buffer.push_back(VideoSequenceBufferEntry::new(Vec::from(data.payload), sample_ctx));

                                        let front = self.video_sequence_buffer.pop_front();
                                        if let Some(mut front) = front {
                                            let mut send_data = Encoder::encode_moof(&mut self.ctx, &mut self.video_track, &mut front.sample_ctx).serialize();
                                            send_data.append(&mut Encoder::encode_mdat(Vec::from(front.payload)).serialize());
                                            self.send_raw_data(RemuxedData::Video(send_data))?;
                                        }
                                    } else {
                                        self.video_dts_adjust = Some(parse_timescale(tag.timestamp));

                                        let sample_ctx = SampleContextBuilder::new()
                                            .set_decode_time(parse_timescale(tag.timestamp))
                                            .set_sample_size(data.payload.len() as u32)
                                            .set_sample_duration(parse_avc_timescale(self.ctx.fps as f32))
                                            .set_composition_time_offset(0)
                                            .set_has_redundancy(false)
                                            .set_is_leading(self.video_track.sequence_number == 1)
                                            .set_is_keyframe(data.keyframe_type == KeyframeType::Keyframe)
                                            .set_is_non_sync(data.keyframe_type == KeyframeType::Interframe)
                                            .build();

                                        self.video_sequence_buffer.push_back(VideoSequenceBufferEntry::new(Vec::from(data.payload), sample_ctx));
                                    }
                                }
                                Avc1ParseResult::AvcSequenceHeader(_) => {
                                    panic!("[Remuxer] Avc sequence header not set!")
                                }
                                Avc1ParseResult::AvcEndOfSequence => {
                                    // todo: handle all the remaining frames in the buffer.
                                    while !self.video_sequence_buffer.is_empty() {
                                        let entry = self.video_sequence_buffer.pop_front().unwrap();
                                        let mut sample_ctx = entry.sample_ctx;
                                        let mut send_data = Encoder::encode_moof(&mut self.ctx, &mut self.video_track, &mut sample_ctx).serialize();
                                        send_data.append(&mut Encoder::encode_mdat(entry.payload).serialize());
                                        self.send_raw_data(RemuxedData::Video(send_data))?;
                                        self.frame_count += 1;
                                    }
                                    println!("[Remuxer] End of sequence.");
                                    println!("[Remuxer] Frame count: {}", self.frame_count)
                                }
                            }
                        }
                    } else {
                        println!("[Remuxer] Parsed video tag.");
                        if let Some(conf) = self.ctx.configure_video_metadata(&parsed) {
                            self.send(
                                Packed {
                                    packed_routing: Destination::Core,
                                    packed_content: PackedContent::ToCore(
                                        PackedContentToCore::DecoderConfig(
                                            MseDecoderConfig::VideoCodec(conf)
                                        )
                                    )
                                }
                            )?;
                        }
                    }
                }
                TagType::Script => {}
                TagType::Encryption => {}
            }
        }

        Ok(())
    }

    pub fn push_pack(&mut self, pack: Packed) {
        self.pack_buffer.push_back(pack);
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(received) = self.pack_buffer.pop_front() {
            if received.packed_routing != Destination::Remuxer {
                self.core.push_pack(received);
                continue;
            }
            if let PackedContent::ToRemuxer(content) = received.packed_content {
                match content {
                    PackedContentToRemuxer::PushTag(tag) => {
                        // println!("Pushed tag.");
                        self.tags.push_back(tag);
                    }
                    PackedContentToRemuxer::PushFlvHeader(flv_header) => {
                        println!("[Remuxer] Pushed flv header.");
                        self.ctx.parse_flv_header(&flv_header);
                        self.flv_header = Some(flv_header);
                    }
                    PackedContentToRemuxer::PushMetadata(metadata) => {
                        println!("[Remuxer] Pushed metadata.");
                        self.ctx.parse_metadata(&metadata);
                        self.metadata = Some(metadata);
                    }
                    PackedContentToRemuxer::StartRemuxing => {
                        println!("[Remuxer] Start remuxing.");
                        self.set_remuxing(true)
                    }
                    PackedContentToRemuxer::StopRemuxing => {
                        println!("[Remuxer] Stop remuxing.");
                        self.set_remuxing(false)
                    }
                    PackedContentToRemuxer::CloseWorkerThread => {
                        println!("[Remuxer] Closing remuxer thread.");
                        return Ok(());
                    }
                    PackedContentToRemuxer::Now => { }
                }
            }
        }

        if !self.remuxing {
            return Ok(())
        }

        if self.ctx.is_metadata_complete() {
            if self.remux().is_err() {
                println!("[Remuxer] Remux error.");
                return Ok(())
            }
        } else {
            println!("[Remuxer] Not configured yet.");
        }
        self.core.process_incoming()?;
        Ok(())
    }

    pub fn launch_worker_thread(mut self) -> JoinHandle<()> {
        std::thread::spawn(move || {
            if let Err(e) = self.run() {
                panic!("Remuxer worker thread stopped unexpectedly: {}", e)
            }
        })
    }
}