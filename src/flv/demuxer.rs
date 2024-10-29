use crate::exchange::{Destination, Packed, PackedContent, PackedContentToDemuxer, PackedContentToRemuxer};
use crate::flv::header::FlvHeader;
use crate::flv::meta::RawMetaData;
use crate::flv::tag::{NormalTagBody, Tag, TagBody, TagType};
use std::collections::VecDeque;
use std::thread::JoinHandle;
use crate::fmpeg::remuxer::Remuxer;

pub struct Demuxer {
    pack_buffer: VecDeque<Packed>,
    demuxing: bool,
    pub remuxer: Remuxer,

    cache_media_tags: VecDeque<Tag>,
    cache_script_tags: VecDeque<Tag>,
    cache_metadata: Option<RawMetaData>,
    cache_flv_header: Option<FlvHeader>,
}

impl Demuxer {
    pub fn new() -> Self {
        Self {
            pack_buffer: VecDeque::new(),
            demuxing: false,
            remuxer: Remuxer::new(),
            cache_media_tags: VecDeque::new(),
            cache_script_tags: VecDeque::new(),
            cache_metadata: None,
            cache_flv_header: None,
        }
    }

    fn set_demuxing(&mut self, flag: bool) {
        self.demuxing = flag;
    }

    fn process_incoming_tag(&mut self, tag: Tag) {
        if tag.tag_type != TagType::Script {
            match tag.tag_type {
                TagType::Audio => self.cache_media_tags.push_back(tag),
                TagType::Video => self.cache_media_tags.push_back(tag),
                _ => {}
            }
        } else {
            if let TagBody::Normal(ref normal) = tag.tag_body {
                if let NormalTagBody::Script(script) = normal {
                    if script.name.data == "onMetaData" {
                        self.cache_metadata = Some(RawMetaData::new(script.clone()));
                        return;
                    }
                }
            }
            self.cache_script_tags.push_back(tag);
        }
    }

    pub fn push_pack(&mut self, pack: Packed) {
        self.pack_buffer.push_back(pack);
    }

    fn send_to_remuxer(&mut self, pack: Packed) -> Result<(), Box<dyn std::error::Error>> {
        self.remuxer.push_pack(pack);
        Ok(())
    }

    fn send_from_cache(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(flv_header) = self.cache_flv_header.take() {
            let pack = Packed {
                packed_routing: Destination::Remuxer,
                packed_content: PackedContent::ToRemuxer(PackedContentToRemuxer::PushFlvHeader(flv_header)),
            };
            self.send_to_remuxer(pack)?;
        }

        if let Some(metadata) = self.cache_metadata.take() {
            let pack = Packed {
                packed_routing: Destination::Remuxer,
                packed_content: PackedContent::ToRemuxer(PackedContentToRemuxer::PushMetadata(metadata)),
            };
            self.send_to_remuxer(pack)?;
        }

        while let Some(audio) = self.cache_media_tags.pop_front() {
            let pack = Packed {
                packed_routing: Destination::Remuxer,
                packed_content: PackedContent::ToRemuxer(PackedContentToRemuxer::PushTag(audio)),
            };
            self.send_to_remuxer(pack)?;
        }

        while let Some(script) = self.cache_script_tags.pop_front() {
            let pack = Packed {
                packed_routing: Destination::Remuxer,
                packed_content: PackedContent::ToRemuxer(PackedContentToRemuxer::PushTag(script)),
            };
            self.send_to_remuxer(pack)?;
        }

        Ok(())
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(received) = self.pack_buffer.pop_front() {
            if received.packed_routing != Destination::Demuxer {
                self.remuxer.push_pack(received);
                continue;
            }
            match received.packed_content {
                PackedContent::ToDemuxer(content) => {
                    match content {
                        PackedContentToDemuxer::PushTag(tag) => {
                            // todo: implement tag processing.
                            self.process_incoming_tag(tag);
                        }
                        PackedContentToDemuxer::PushFlvHeader(flv_header) => {
                            println!("[Demuxer] Received flv header.");
                            self.cache_flv_header = Some(flv_header);
                        }
                        PackedContentToDemuxer::StartDemuxing => {
                            println!("[Demuxer] Start demuxing.");
                            self.set_demuxing(true);
                        }
                        PackedContentToDemuxer::StopDemuxing => {
                            println!("[Demuxer] Stop demuxing.");
                            self.set_demuxing(false);
                        }
                        PackedContentToDemuxer::CloseWorkerThread => {
                            println!("[Demuxer] Close worker thread.");
                            return Ok(());
                        }
                        PackedContentToDemuxer::Now => {
                            // just to temporarily remove thread blockage.
                        }
                    }
                }
                _ => {}
            }
        }

        if !self.demuxing {
            return Ok(());
        }

        self.send_from_cache()?;
        self.remuxer.run()?;
        Ok(())
    }

    /// Launch a worker thread, move the self into it.
    /// Note that the data stream will not be sent unless the StartDemuxing command is sent.
    pub fn launch_worker_thread(mut self) -> JoinHandle<()> {
        std::thread::spawn(move || {
            if let Err(e) = self.run() {
                panic!("Demuxer worker thread stopped unexpectedly: {}", e);
            }
        })
    }
}