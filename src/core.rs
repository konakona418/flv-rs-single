use crate::exchange::{AudioCodecConfig, MseDecoderConfig, Packed, PackedContent, PackedContentToCore, RemuxedData, VideoCodecConfig};
use std::collections::VecDeque;

pub struct Core {
    pub buffer: VecDeque<RemuxedData>,
    pub pack_buffer: VecDeque<Packed>,

    audio_codec_conf: Option<AudioCodecConfig>,
    video_codec_conf: Option<VideoCodecConfig>,
}

impl Core {
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::new(),
            pack_buffer: VecDeque::new(),
            audio_codec_conf: None,
            video_codec_conf: None,
        }
    }

    pub fn push_pack(&mut self, pack: Packed) {
        self.pack_buffer.push_back(pack);
    }

    pub fn process_incoming(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while let Some(data) = self.pack_buffer.pop_front() {
            match data.packed_content {
                PackedContent::ToCore(PackedContentToCore::Data(data)) => {
                    self.buffer.push_back(data);
                }
                PackedContent::ToCore(PackedContentToCore::DecoderConfig(conf)) => {
                    match conf {
                        MseDecoderConfig::AudioCodec(audio_codec) => {
                            self.audio_codec_conf = Some(audio_codec);
                        }
                        MseDecoderConfig::VideoCodec(video_codec) => {
                            self.video_codec_conf = Some(video_codec);
                        }
                    }
                }
                _ => {}
            };
        };
        Ok(())
    }
}

impl IConsumable for Core {
    type ConsumerData = RemuxedData;

    fn consume(&mut self) -> Result<RemuxedData, Box<dyn std::error::Error>> {
        self.process_incoming()?;

        if let Some(data) = self.buffer.pop_front() {
            Ok(data)
        } else {
            Err("No data available".into())
        }
    }
}

impl Core {
    pub fn get_audio_codec_conf(&mut self) -> Option<String> {
        match self.audio_codec_conf {
            Some(ref mut conf) => Some(conf.audio_conf()),
            None => None
        }
    }

    pub fn get_video_codec_conf(&mut self) -> Option<String> {
        match self.video_codec_conf {
            Some(ref mut conf) => Some(conf.video_conf()),
            None => None
        }
    }

    pub fn is_codec_configured(&self) -> bool {
        self.audio_codec_conf.is_some() && self.video_codec_conf.is_some()
    }

    /// Returns the codec configuration if it is already set
    /// Returns a tuple of audio and video codec configuration in String.
    /// If the codec configuration is not set, returns None.
    /// This method will not block.
    pub fn try_get_codec_conf(&mut self) -> Option<(String, String)> {
        if self.is_codec_configured() {
            return Some((self.get_audio_codec_conf()?, self.get_video_codec_conf()?));
        }
        None
    }

    /// Returns the codec configuration. This method will block until the codec configuration is ready.
    pub fn get_codec_conf(&mut self) -> Result<(String, String), Box<dyn std::error::Error>> {
        // todo: [OPTIMIZATION REQUIRED] this is a blocking call. use try_get_codec_conf instead.
        self.process_incoming()?;

        loop {
            match self.try_get_codec_conf() {
                Some(conf) => return Ok(conf),
                None => {}
            }
        }
    }

    /// Returns the codec configuration with a timeout.
    pub fn get_codec_conf_with_timeout(&mut self, timeout: std::time::Duration) -> Result<(String, String), Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            match self.try_get_codec_conf() {
                Some(conf) => return Ok(conf),
                None => {}
            }
        };
        Err("Unable to get codec configuration. Time limit exceeded. ".into())
    }

    /// Returns the codec configuration with a default value if the codec configuration is not ready.
    /// Note that the return value may not be guaranteed to be valid.
    /// If the validity cannot be guaranteed, the result will be wrapped in an Err.
    /// If the validity can be guaranteed, the result will be wrapped in an Ok.
    pub fn get_codec_conf_or_default(&mut self) -> Result<(String, String), (String, String)> {
        match self.try_get_codec_conf() {
            Some(conf) => return Ok(conf),
            None => {}
        };
        Err(("mp4a.40.2".into(), "avc1.64001E".into()))
    }
}

pub trait IConsumable {
    type ConsumerData;
    fn consume(&mut self) -> Result<Self::ConsumerData, Box<dyn std::error::Error>>;
}

