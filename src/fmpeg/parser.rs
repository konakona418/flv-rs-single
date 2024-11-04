use crate::flv::header::{AudioTagHeader, TagHeader, VideoTagHeader};
use crate::flv::tag::{NormalTagBody, Tag, TagBody};
use crate::fmpeg::remux_context::TIME_SCALE;
use crate::io;
use std::collections::VecDeque;

#[inline]
pub fn parse_timescale(timestamp_ms: u32) -> u32 {
    /*if TIME_SCALE == 1000 {
        timestamp_ms
    } else {
        // this may lead to overflow.
        timestamp_ms * TIME_SCALE / 1000
    }*/

    // Note: no more parse_timescale()
    // it may lead to accuracy issue.
    parse_timescale_accurate(timestamp_ms as f32)
}

#[inline]
pub fn parse_timescale_accurate(timestamp_ms: f32) -> u32 {
    if TIME_SCALE == 1000 {
        timestamp_ms as u32
    } else {
        (timestamp_ms * TIME_SCALE as f32 / 1000.0) as u32
    }
}

#[inline]
pub fn parse_timescale_signed(timestamp_ms: i32) -> i32 {
    if TIME_SCALE == 1000 {
        timestamp_ms
    } else {
        (timestamp_ms as f32 * TIME_SCALE as f32 / 1000.0) as i32
    }
}

#[inline]
pub fn parse_mp3_timescale(sample_rate: u32, mp3version: Mp3Version) -> u32 {
    match mp3version {
        Mp3Version::Mp25 => {
            parse_timescale_accurate(576000.0 / sample_rate as f32)
        }
        Mp3Version::Mp20 => {
            parse_timescale_accurate(576000.0 / sample_rate as f32)
        }
        Mp3Version::Mp10 => {
            parse_timescale_accurate(1152000.0 / sample_rate as f32)
        }
        Mp3Version::Reserved => {
            panic!("Invalid mp3 version.");
        }
    }
}

#[inline]
pub fn parse_aac_timescale(sample_rate: u32) -> u32 {
    // this may be incorrect.
    parse_timescale_accurate((1024.0 * 1000.0) / sample_rate as f32)
}

#[inline]
pub fn parse_avc_timescale(fps: f32) -> u32 {
    parse_timescale_accurate(1000.0 / fps)
}

pub enum AudioParseResult {
    AacRaw(VecDeque<u8>),
    AacSequenceHeader(AacSequenceHeader),
    Mp3(Mp3ParseResult),
}

pub enum VideoParseResult {
    Avc1(Avc1ParseResult),
}

pub enum Avc1ParseResult {
    AvcNalu(AvcNalu),
    AvcSequenceHeader(VecDeque<u8>),
    AvcEndOfSequence,
}

pub struct AvcNalu {
    pub keyframe_type: KeyframeType,
    pub payload: VecDeque<u8>,
}

pub enum KeyframeType {
    Keyframe,
    Interframe,
}

impl From<u8> for KeyframeType {
    /// for conversion from flv tag only.
    fn from(value: u8) -> Self {
        match value {
            1 => KeyframeType::Keyframe,
            2 => KeyframeType::Interframe,
            _ => panic!("Invalid keyframe type."),
        }
    }
}

pub enum AudioConfigurationLike {
    Mp3(Mp3ParseResult),
    Aac(AacSequenceHeader),
}

pub enum Mp3Version {
    Mp25,
    Mp20,
    Mp10,
    Reserved,
}

impl From<u8> for Mp3Version {
    fn from(value: u8) -> Self {
        match value {
            0 => Mp3Version::Mp25,
            1 => Mp3Version::Reserved,
            2 => Mp3Version::Mp20,
            3 => Mp3Version::Mp10,
            _ => panic!("Invalid mp3 version."),
        }
    }
}

pub enum Mp3Layer {
    Reserved,
    L1,
    L2,
    L3,
}

impl From<u8> for Mp3Layer {
    fn from(value: u8) -> Self {
        match value {
            0 => Mp3Layer::Reserved,
            1 => Mp3Layer::L3,
            2 => Mp3Layer::L2,
            3 => Mp3Layer::L1,
            _ => panic!("Invalid mp3 layer."),
        }
    }
}

pub enum Channel {
    Mono,
    Dual,
    Stereo,
    JointStereo,
}

impl From<u8> for Channel {
    fn from(value: u8) -> Self {
        match value {
            0 => Channel::Stereo,
            1 => Channel::JointStereo,
            2 => Channel::Dual,
            3 => Channel::Mono,
            _ => panic!("Invalid channel."),
        }
    }
}

pub struct Mp3ParseResult {
    pub version: Mp3Version,
    pub layer: Mp3Layer,
    pub sample_rate: u32,
    pub bitrate: u32,
    pub channel: Channel,
    pub channel_extended: u8,

    pub body: Vec<u8>,
}

pub const AUDIO_SAMPLE_RATE_TABLE_M10: [u32; 4] = [44100, 48000, 32000, 0];
pub const AUDIO_SAMPLE_RATE_TABLE_M20: [u32; 4] = [22050, 24000, 16000, 0];
pub const AUDIO_SAMPLE_RATE_TABLE_M25: [u32; 4] = [11025, 12000, 8000, 0];

pub const AUDIO_BITRATE_TABLE_L1: [u32; 16] = [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0];
pub const AUDIO_BITRATE_TABLE_L2: [u32; 16] = [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0];
pub const AUDIO_BITRATE_TABLE_L3: [u32; 16] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0];

const MP3_SYNC_WORD: u16 = 0x07FF;

pub struct AacSequenceHeader {
    pub audio_object_type: u8,
    pub sampling_frequency_index: u8,
    pub channel_configuration: u8,
    pub raw: VecDeque<u8>,
}

pub struct Parser;

impl Parser {
    pub fn parse_audio(tag: &Tag) -> Result<AudioParseResult, Box<dyn std::error::Error>> {
        let header = match tag.tag_header {
            TagHeader::Audio(ref header) => header,
            _ => return Err("Tag type mismatch.".into()),
        };

        let body = match tag.tag_body {
            TagBody::Normal(ref body) =>
                match body {
                    NormalTagBody::Audio(ref body) => { body }
                    _ => return Err("Tag body type mismatch.".into()),
                },
            _ => return Err("Encrypted audio is not supported.".into()),
        };

        // mp3; aac
        if header.sound_format != 2 && header.sound_format != 10 {
            return Err("Unsupported sound format.".into());
        }

        if header.sound_format == 2 {
            // mp3
            Self::parse_mp3(header, body)
        } else {
            // aac
            Self::parse_aac(header, body)
        }
    }

    fn parse_mp3(header: &AudioTagHeader, body: &VecDeque<u8>) -> Result<AudioParseResult, Box<dyn std::error::Error>> {
        let mut u16io = io::bit::U16BitIO::new(
            <u16>::from_be_bytes(
                [
                    body[0],
                    body[1]
                ]
            ),
            io::bit::UIntParserEndian::BigEndian,
        );
        // dbg!(u16io.data);

        let sync_word = u16io.read_range(0, 10);
        if sync_word != MP3_SYNC_WORD {
            // dbg!(sync_word);
            return Err("MP3 sync word mismatch!".into());
        }

        let version = Mp3Version::from(u16io.read_range(11, 12) as u8);
        let layer = Mp3Layer::from(u16io.read_range(13, 14) as u8);
        let protection_bit = u16io.read_at(15);

        let mut u16io = io::bit::U16BitIO::new(
            <u16>::from_be_bytes(
                [
                    body[2],
                    body[3]
                ]
            ),
            io::bit::UIntParserEndian::BigEndian,
        );
        let bitrate_index = u16io.read_range(0, 3);
        let sampling_rate_index = u16io.read_range(4, 5);
        // 1 bit padding.
        let _ = u16io.read_range(6, 7);
        let channel_mode = u16io.read_range(8, 9);

        let sample_rate = match version {
            Mp3Version::Mp25 => AUDIO_SAMPLE_RATE_TABLE_M25[sampling_rate_index as usize],
            Mp3Version::Mp20 => AUDIO_SAMPLE_RATE_TABLE_M20[sampling_rate_index as usize],
            Mp3Version::Mp10 => AUDIO_SAMPLE_RATE_TABLE_M10[sampling_rate_index as usize],
            _ => panic!("Invalid mp3 version."),
        };

        let bitrate = match layer {
            Mp3Layer::L1 => AUDIO_BITRATE_TABLE_L1[bitrate_index as usize],
            Mp3Layer::L2 => AUDIO_BITRATE_TABLE_L2[bitrate_index as usize],
            Mp3Layer::L3 => AUDIO_BITRATE_TABLE_L3[bitrate_index as usize],
            _ => panic!("Invalid mp3 layer."),
        };
        // todo: is this okay?

        let channel = Channel::from(channel_mode as u8);
        let channel_extended: u8;
        if let Channel::JointStereo = channel {
            channel_extended = u16io.read_range(10, 11) as u8;
        } else {
            channel_extended = 0;
        }

        Ok(AudioParseResult::Mp3(Mp3ParseResult {
            version,
            layer,
            sample_rate,
            bitrate,
            channel,
            channel_extended,
            body: Vec::from(body.clone()),
        }))
    }

    fn parse_aac(header: &AudioTagHeader, body: &VecDeque<u8>) -> Result<AudioParseResult, Box<dyn std::error::Error>> {
        if let Some(aac_pack_type) = header.aac_packet_type {
            match aac_pack_type {
                0 => Self::parse_aac_seq_hdr(body),
                1 => Self::parse_aac_raw(body),
                _ => Err("Unsupported AAC packet type.".into()),
            }
        } else {
            Err("AAC packet type is not set.".into())
        }
    }

    fn parse_aac_seq_hdr(body: &VecDeque<u8>) -> Result<AudioParseResult, Box<dyn std::error::Error>> {
        let mut u16io = io::bit::U16BitIO::new(
            <u16>::from_be_bytes(
                [
                    body[0],
                    body[1]
                ]
            ),
            io::bit::UIntParserEndian::BigEndian,
        );
        let audio_object_type = u16io.read_range(0, 4) as u8;
        let sampling_frequency_index = u16io.read_range(5, 8) as u8;
        let channel_configuration = u16io.read_range(9, 12) as u8;
        // todo: note that this is just a temporary solution and requires optimizing.

        Ok(AudioParseResult::AacSequenceHeader(AacSequenceHeader {
            audio_object_type,
            sampling_frequency_index,
            channel_configuration,
            raw: body.clone(),
        }))
    }

    fn parse_aac_raw(body: &VecDeque<u8>) -> Result<AudioParseResult, Box<dyn std::error::Error>> {
        Ok(AudioParseResult::AacRaw(body.clone()))
    }

    pub fn parse_video(tag: &Tag) -> Result<VideoParseResult, Box<dyn std::error::Error>> {
        let header = match tag.tag_header {
            TagHeader::Video(ref header) => header,
            _ => return Err("Tag type mismatch.".into()),
        };

        let body = match tag.tag_body {
            TagBody::Normal(ref body) => {
                match body {
                    NormalTagBody::Video(body) => body,
                    _ => return Err("Tag body type mismatch.".into()),
                }
            }
            _ => return Err("Encrypted video is not supported.".into()),
        };

        if header.codec_id == 7 {
            // h264 avc
            Self::parse_avc(header, body)
        } else {
            Err("Unsupported video codec.".into())
        }
    }

    fn parse_avc(header: &VideoTagHeader, body: &VecDeque<u8>) -> Result<VideoParseResult, Box<dyn std::error::Error>> {
        match header.avc_packet_type {
            None => Err("AVC packet type is not set.".into()),
            Some(pack_type) => {
                match pack_type {
                    // todo: use something instead of cloning.
                    0 => Ok(VideoParseResult::Avc1(Avc1ParseResult::AvcSequenceHeader(body.clone()))),
                    1 => Ok(VideoParseResult::Avc1(Avc1ParseResult::AvcNalu(Self::parse_avc_nalu(header, body.clone())?))),
                    2 => Ok(VideoParseResult::Avc1(Avc1ParseResult::AvcEndOfSequence)),
                    _ => Err("Unsupported AVC packet type.".into()),
                }
            }
        }
    }

    fn parse_avc_nalu(header: &VideoTagHeader, mut payload: VecDeque<u8>) -> Result<AvcNalu, Box<dyn std::error::Error>> {
        // todo: [IMPORTANT] this is a simplified solution and requires further optimization.
        // although codec of most modern browsers can identify an fix the mismatch between the header and the actual data,
        // and simply use flv header to determine the keyframe type will not cause obvious problems,
        // however it's still unsafe and may lead to unexpected errors.
        // use payload of the AVC NAL unit to determine whether the frame is a keyframe or not.
        // use flv header as a reference only.

        let size = payload.len() as u32;
        let nalu_type = KeyframeType::from(header.frame_type);

        if size != 0x00000001 { // start code not present
            Ok(AvcNalu {
                keyframe_type: nalu_type,
                payload,
            })
        } else { // start code present
            let mut u32io = io::bit::U32BitIO::new(size, io::bit::UIntParserEndian::BigEndian);
            u32io.write_range(0, 31, size);
            let data = u32io.get_data();

            // convert the first 4 bytes to the chunk size.
            payload[0] = data[0];
            payload[1] = data[1];
            payload[2] = data[2];
            payload[3] = data[3];

            Ok(AvcNalu {
                keyframe_type: nalu_type,
                payload,
            })
        }
    }
}