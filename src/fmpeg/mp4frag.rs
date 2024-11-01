use crate::fmpeg::mp4head::ISerializable;
use crate::fmpeg::mp4head::U24;

pub struct MovieFragmentBox<T: ISerializable + ITrackFragmentBoxLike> {
    pub size: u32,
    pub box_type: [char; 4],

    pub movie_fragment_header_box: MovieFragmentHeaderBox,
    pub track_fragment_box: T,
}

impl<T> MovieFragmentBox<T> where T: ISerializable + ITrackFragmentBoxLike {
    pub fn new(sequence_number: u32, track_fragment_box: T) -> MovieFragmentBox<T> {
        MovieFragmentBox {
            size: 0,
            box_type: ['m', 'o', 'o', 'f'],
            movie_fragment_header_box: MovieFragmentHeaderBox::new(sequence_number),
            track_fragment_box,
        }
    }

    pub fn deferred_set_trun_size(&mut self) {
        let size= self.size();
        assert_ne!(size, 0);
        self.track_fragment_box.deferred_set_data_offset(size + 8); // Magic!!
    }
}

impl<T> ISerializable for MovieFragmentBox<T> where T: ISerializable + ITrackFragmentBoxLike {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));

        result.extend_from_slice(&self.movie_fragment_header_box.serialize());
        result.extend_from_slice(&self.track_fragment_box.serialize());
        result
    }

    fn size(&self) -> u32 {
        8 + self.movie_fragment_header_box.size() + self.track_fragment_box.size()
    }
}

pub struct MovieFragmentHeaderBox {
    pub size: u32,
    pub box_type: [char; 4],
    pub version: u8,
    pub flags: U24,

    pub sequence_number: u32,
}

impl MovieFragmentHeaderBox {
    pub fn new(sequence_number: u32) -> MovieFragmentHeaderBox {
        MovieFragmentHeaderBox {
            size: 0,
            box_type: ['m', 'f', 'h', 'd'],
            version: 0,
            flags: U24::from(0),
            sequence_number
        }
    }
}

impl ISerializable for MovieFragmentHeaderBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));

        result.push(self.version);
        result.extend_from_slice(&self.flags.serialize());
        result.extend_from_slice(&self.sequence_number.to_be_bytes());
        assert_eq!(result.len(), 16);

        result
    }

    fn size(&self) -> u32 {
        16
    }
}

pub trait ITrackFragmentBoxLike {
    fn deferred_set_data_offset(&mut self, data_offset: u32);
}

#[derive(Debug)]
pub struct TrackFragmentBox {
    pub size: u32,
    pub box_type: [char; 4],

    pub track_fragment_header_box: TrackFragmentHeaderBox,
    pub track_fragment_decode_time_box: TrackFragmentDecodeTimeBox,
    pub sample_table_box: SampleDependencyTableBox,
    pub track_run_box: TrackRunBox,
}

pub struct TrackFragmentBoxBuilder {
    pub track_fragment_header_box: TrackFragmentHeaderBox,
    pub track_fragment_decode_time_box: TrackFragmentDecodeTimeBox,
    pub sample_table_box: SampleDependencyTableBox,
    pub track_run_box: TrackRunBox,
}

impl ITrackFragmentBoxLike for TrackFragmentBox {
    fn deferred_set_data_offset(&mut self, data_offset: u32) {
        self.track_run_box.data_offset = data_offset;
    }
}

impl TrackFragmentBoxBuilder {
    pub fn new() -> TrackFragmentBoxBuilder {
        TrackFragmentBoxBuilder {
            track_fragment_header_box: TrackFragmentHeaderBox::new(1),
            track_fragment_decode_time_box: TrackFragmentDecodeTimeBox::new(0),
            sample_table_box: SampleDependencyTableBoxBuilder::Audio.as_box(),
            track_run_box: TrackRunBox::new(),
        }
    }

    pub fn with_track_id(mut self, track_id: u32) -> TrackFragmentBoxBuilder {
        self.track_fragment_header_box.track_id = track_id;
        self
    }

    pub fn with_media_decode_time(mut self, base_media_decode_time: u32) -> TrackFragmentBoxBuilder {
        self.track_fragment_decode_time_box.base_media_decode_time = base_media_decode_time;
        self
    }

    pub fn with_sample_options(mut self, sample_flag_builder: SampleFlagBuilder) -> TrackFragmentBoxBuilder {
        self.track_run_box.sample_flags = sample_flag_builder.build();
        self
    }

    pub fn with_sample_table_box(mut self, sample_table_box: SampleDependencyTableBoxBuilder) -> TrackFragmentBoxBuilder {
        self.sample_table_box = sample_table_box.as_box();
        self
    }

    pub fn with_track_run_box(mut self, track_run_box: TrackRunBox) -> TrackFragmentBoxBuilder {
        self.track_run_box = track_run_box;
        self
    }

    pub fn build(self) -> TrackFragmentBox {
        TrackFragmentBox {
            size: 0,
            box_type: ['t', 'r', 'a', 'f'],
            track_fragment_header_box: self.track_fragment_header_box,
            track_fragment_decode_time_box: self.track_fragment_decode_time_box,
            sample_table_box: self.sample_table_box,
            track_run_box: self.track_run_box
        }
    }
}

impl ISerializable for TrackFragmentBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));

        result.extend_from_slice(&self.track_fragment_header_box.serialize());
        result.extend_from_slice(&self.track_fragment_decode_time_box.serialize());

        // self.track_run_box.data_offset = self.size();

        // todo: something wrong with this.
        result.extend_from_slice(&self.track_run_box.serialize());

        result.extend_from_slice(&self.sample_table_box.serialize());

        result
    }

    fn size(&self) -> u32 {
        8 + self.track_fragment_header_box.size()
            + self.track_fragment_decode_time_box.size()
            + self.sample_table_box.size()
            + self.track_run_box.size()
    }
}

#[derive(Debug)]
pub struct TrackFragmentHeaderBox {
    pub size: u32,
    pub box_type: [char; 4],
    pub version: u8,
    pub flags: U24,

    pub track_id: u32,
}

impl TrackFragmentHeaderBox {
    pub fn new(track_id: u32) -> TrackFragmentHeaderBox {
        TrackFragmentHeaderBox {
            size: 0,
            box_type: ['t', 'f', 'h', 'd'],
            version: 0,
            flags: U24::from(0),
            track_id,
        }
    }
}

impl ISerializable for TrackFragmentHeaderBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));

        result.push(self.version);
        result.extend_from_slice(&self.flags.serialize());

        result.extend_from_slice(&self.track_id.to_be_bytes());
        assert_eq!(result.len(), 16);
        result
    }

    fn size(&self) -> u32 {
        16
    }
}

#[derive(Debug)]
pub struct TrackFragmentDecodeTimeBox {
    pub size: u32,
    pub box_type: [char; 4],
    pub version: u8,
    pub flags: U24,

    pub base_media_decode_time: u32,
}

impl TrackFragmentDecodeTimeBox {
    pub fn new(base_media_decode_time: u32) -> TrackFragmentDecodeTimeBox {
        TrackFragmentDecodeTimeBox {
            size: 0,
            box_type: ['t', 'f', 'd', 't'],
            version: 0,
            flags: U24::from(0),
            base_media_decode_time
        }
    }
}

impl ISerializable for TrackFragmentDecodeTimeBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));

        result.push(self.version);
        result.extend_from_slice(&self.flags.serialize());

        result.extend_from_slice(&self.base_media_decode_time.to_be_bytes());
        assert_eq!(result.len(), 16);
        result
    }

    fn size(&self) -> u32 {
        16
    }
}

#[derive(Debug)]
pub struct SampleDependencyTableBox {
    pub size: u32,
    pub box_type: [char; 4],
    pub version: u8,
    pub flags: U24,

    pub sample_dependency_flags: u8,
}

impl SampleDependencyTableBox {
    pub fn new(sample_dependency_flags: u8) -> SampleDependencyTableBox {
        SampleDependencyTableBox {
            size: 0,
            box_type: ['s', 'd', 't', 'p'],
            version: 0,
            flags: U24::from(0),
            sample_dependency_flags
        }
    }
}

impl ISerializable for SampleDependencyTableBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));

        result.push(self.version);
        result.extend_from_slice(&self.flags.serialize());

        result.push(self.sample_dependency_flags);
        // todo: this is a simplified version which supports only one sample per sample entry.
        assert_eq!(result.len(), 13);
        result
    }

    fn size(&self) -> u32 {
        13
    }
}

pub enum SampleDependencyTableBoxBuilder {
    VideoKeyFrame,
    VideoInterFrame,
    Audio,
}

impl SampleDependencyTableBoxBuilder {
    pub fn as_box(&self) -> SampleDependencyTableBox {
        match self {
            SampleDependencyTableBoxBuilder::VideoKeyFrame => SampleDependencyTableBox::new(0x18),
            SampleDependencyTableBoxBuilder::VideoInterFrame => SampleDependencyTableBox::new(0x24),
            SampleDependencyTableBoxBuilder::Audio => SampleDependencyTableBox::new(0x10),
        }
    }
}

impl SampleDependencyTableBoxBuilder {
    pub fn get_flags(&self) -> U24 {
        match self {
            SampleDependencyTableBoxBuilder::VideoKeyFrame => U24::from(0x18),
            SampleDependencyTableBoxBuilder::VideoInterFrame => U24::from(0x24),
            SampleDependencyTableBoxBuilder::Audio => U24::from(0x10),
        }
    }
}

impl ISerializable for SampleDependencyTableBoxBuilder {
    fn serialize(&mut self) -> Vec<u8> {
        let mut sd: SampleDependencyTableBox = match self {
            // this is a hack for now.
            // todo: this may trigger some issues in the future.
            SampleDependencyTableBoxBuilder::VideoKeyFrame => SampleDependencyTableBox::new(0x18),
            SampleDependencyTableBoxBuilder::VideoInterFrame => SampleDependencyTableBox::new(0x24),
            SampleDependencyTableBoxBuilder::Audio => SampleDependencyTableBox::new(0x10),
        };
        sd.serialize()
    }

    fn size(&self) -> u32 {
        // todo: this is a simplified version which supports only one sample per sample entry.
        13
    }
}


/// for trun box only
pub struct SampleFlagBuilder {
    pub is_leading: bool,
    pub sample_depends_on: bool,
    pub sample_is_depended_on: bool,
    pub sample_has_redundancy: bool,
    pub is_non_sync: bool,
}

impl SampleFlagBuilder {
    pub fn new() -> SampleFlagBuilder {
        SampleFlagBuilder {
            is_leading: false,
            sample_depends_on: false,
            sample_is_depended_on: false,
            sample_has_redundancy: false,
            is_non_sync: false,
        }
    }

    pub fn set_is_leading(mut self, is_leading: bool) -> SampleFlagBuilder {
        self.is_leading = is_leading;
        self
    }

    /// for keyframes, set this to false
    /// for inter frames, set this to true
    pub fn set_sample_depends_on(mut self, sample_depends_on: bool) -> SampleFlagBuilder {
        self.sample_depends_on = sample_depends_on;
        self
    }

    /// for keyframes, set this to true
    /// for inter frames, set this to false
    pub fn set_sample_is_depended_on(mut self, sample_is_depended_on: bool) -> SampleFlagBuilder {
        self.sample_is_depended_on = sample_is_depended_on;
        self
    }

    pub fn set_sample_has_redundancy(mut self, sample_has_redundancy: bool) -> SampleFlagBuilder {
        self.sample_has_redundancy = sample_has_redundancy;
        self
    }

    /// for keyframes, set this to true
    /// for inter frames, set this to false
    pub fn set_is_non_sync(mut self, is_non_sync: bool) -> SampleFlagBuilder {
        self.is_non_sync = is_non_sync;
        self
    }

    pub fn build(self) -> u16 {
        // todo: check this
        let mut result = 0;
        if self.is_leading {
            result |= 0x0800;
        }
        if self.sample_depends_on {
            result |= 0x0200;
        } else {
            result |= 0x0100;
        }
        if self.sample_is_depended_on {
            result |= 0x0080;
        } else {
            result |= 0x0040;
        }
        if self.sample_has_redundancy {
            result |= 0x0020;
        } else {
            result |= 0x0000;
        }
        if self.is_non_sync {
            result |= 0x0000;
        } else {
            result |= 0x0001;
        }
        result
    }
}

/// this is just a simple implementation
/// which only supports one sample.
#[derive(Debug)]
pub struct TrackRunBox {
    pub size: u32,
    pub box_type: [char; 4],
    pub version: u8,
    pub flags: U24,

    pub sample_count: u32,
    pub data_offset: u32,

    pub sample_duration: u32,
    pub sample_size: u32,
    pub sample_flags: u16,
    pub reserved: u16,
    pub sample_composition_time_offset: i32,
}

impl TrackRunBox {
    pub fn new() -> TrackRunBox {
        TrackRunBox {
            size: 0,
            box_type: ['t', 'r', 'u', 'n'],
            version: 0,
            flags: U24::from(0),

            sample_count: 1,
            data_offset: 0,

            sample_duration: 0,
            sample_size: 0,
            sample_flags: 0,
            reserved: 0,
            sample_composition_time_offset: 0,
        }
    }
}

impl ISerializable for TrackRunBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();
        // dbg!(&self);

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));

        result.push(self.version);
        result.extend_from_slice(&self.flags.serialize());
        result.extend_from_slice(&self.sample_count.to_be_bytes());
        result.extend_from_slice(&self.data_offset.to_be_bytes());

        result.extend_from_slice(&self.sample_duration.to_be_bytes());
        result.extend_from_slice(&self.sample_size.to_be_bytes());
        result.extend_from_slice(&self.reserved.to_be_bytes());
        result.extend_from_slice(&self.sample_flags.to_be_bytes());
        result.extend_from_slice(&self.sample_composition_time_offset.to_be_bytes());
        assert_eq!(result.len(), 36);
        result
    }

    fn size(&self) -> u32 {
        16 + 16 + 4
    }
}

pub struct TrackRunBoxBuilder {
    sample_duration: u32,
    sample_size: u32,
    sample_flags: u16,
    sample_composition_time_offset: i32,
    data_offset: u32,

    flag: u32,
}

impl TrackRunBoxBuilder {
    pub fn new() -> TrackRunBoxBuilder {
        TrackRunBoxBuilder {
            sample_duration: 0,
            sample_size: 0,
            sample_flags: 0,
            sample_composition_time_offset: 0,
            data_offset: 0,

            flag: 0x000F01
            // todo: [TEMPORARY] note that this is just a temporary hack and may cause issues in the future.
        }
    }

    pub fn with_sample_duration(mut self, duration: u32) -> TrackRunBoxBuilder {
        self.sample_duration = duration;
        self.flag |= 0x000100;
        self
    }

    pub fn with_sample_size(mut self, size: u32) -> TrackRunBoxBuilder {
        self.sample_size = size;
        self.flag |= 0x000200;
        self
    }

    pub fn with_sample_flags(mut self, flags: u16) -> TrackRunBoxBuilder {
        self.sample_flags = flags;
        self.flag |= 0x000400;
        self
    }

    pub fn with_sample_composition_time_offset(mut self, offset: i32) -> TrackRunBoxBuilder {
        self.sample_composition_time_offset = offset;
        self.flag |= 0x000800;
        self
    }

    pub fn with_data_offset(mut self, offset: u32) -> TrackRunBoxBuilder {
        self.data_offset = offset;
        self
    }

    pub fn build(self) -> TrackRunBox {
        TrackRunBox {
            size: 0,
            box_type: ['t', 'r', 'u', 'n'],
            version: 0,
            flags: U24::from(self.flag & 0x00FFFFFF),

            sample_count: 1,
            data_offset: 0,

            sample_duration: self.sample_duration,
            sample_size: self.sample_size,
            sample_flags: self.sample_flags,
            reserved: 0,
            sample_composition_time_offset: self.sample_composition_time_offset,
        }
    }
}

pub struct MovieDataBox {
    pub size: u32,
    pub box_type: [char; 4],
    pub data: Vec<u8>,
}

impl ISerializable for MovieDataBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));
        result.extend_from_slice(&self.data);
        assert_ne!(result.len(), 0);
        result
    }

    fn size(&self) -> u32 {
        self.data.len() as u32 + 8
    }
}

impl MovieDataBox {
    pub fn new(data: Vec<u8>) -> MovieDataBox {
        MovieDataBox {
            size: 0,
            box_type: ['m', 'd', 'a', 't'],
            data,
        }
    }

    pub fn add_data(mut self, data: Vec<u8>) -> Self {
        self.data.extend(data);
        self
    }
}

pub struct MergedSampleDependencyTableBox {
    pub size: u32,
    pub box_type: [char; 4],
    pub version: u8,
    pub flags: U24,

    pub entries: Vec<SampleDependencyTableBoxBuilder>
}

impl MergedSampleDependencyTableBox {
    pub fn new() -> MergedSampleDependencyTableBox {
        MergedSampleDependencyTableBox {
            size: 0,
            box_type: ['s', 'd', 't', 'p'],
            version: 0,
            flags: U24::from(0),
            entries: Vec::new()
        }
    }

    pub fn add_entry(mut self, entry: SampleDependencyTableBoxBuilder) -> Self {
        self.entries.push(entry);
        self
    }
}

impl ISerializable for MergedSampleDependencyTableBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));
        result.push(self.version);
        result.extend_from_slice(&self.flags.serialize());
        for entry in self.entries.iter_mut() {
            result.extend(entry.as_box().sample_dependency_flags.to_be_bytes().to_vec());
            self.flags = entry.get_flags();
        }
        assert_ne!(result.len(), 0);
        result
    }

    fn size(&self) -> u32 {
        12 + self.entries.len() as u32
    }
}

pub struct MergedSampleDependencyTableBoxBuilder {
    pub entries: Vec<SampleDependencyTableBoxBuilder>
}

impl MergedSampleDependencyTableBoxBuilder {
    pub fn new() -> MergedSampleDependencyTableBoxBuilder {
        MergedSampleDependencyTableBoxBuilder {
            entries: Vec::new()
        }
    }

    pub fn add_entry(mut self, entry: SampleDependencyTableBoxBuilder) -> Self {
        self.entries.push(entry);
        self
    }

    pub fn build(self) -> MergedSampleDependencyTableBox {
        MergedSampleDependencyTableBox {
            size: 0,
            box_type: ['s', 'd', 't', 'p'],
            version: 0,
            flags: U24::from(0),
            entries: self.entries
        }
    }
}

pub struct MergedTrackRunBoxEntry {
    pub sample_duration: u32,
    pub sample_size: u32,
    pub sample_flags: u16,
    pub reserved: u16,
    pub sample_composition_time_offset: i32,
}

impl MergedTrackRunBoxEntry {
    pub fn new(sample_duration: u32, sample_size: u32, sample_flags: u16, sample_composition_time_offset: i32) -> MergedTrackRunBoxEntry {
        MergedTrackRunBoxEntry {
            sample_duration,
            sample_size,
            sample_flags,
            reserved: 0,
            sample_composition_time_offset,
        }
    }
}

impl ISerializable for MergedTrackRunBoxEntry {
    fn serialize(&mut self) -> Vec<u8> {
        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.sample_duration.to_be_bytes());
        result.extend_from_slice(&self.sample_size.to_be_bytes());
        result.extend_from_slice(&self.sample_flags.to_be_bytes());
        result.extend_from_slice(&self.reserved.to_be_bytes());
        result.extend_from_slice(&self.sample_composition_time_offset.to_be_bytes());
        assert_eq!(result.len(), 16);
        result
    }

    fn size(&self) -> u32 {
        16
    }
}

pub struct MergedTrackRunBoxEntryBuilder {
    sample_duration: u32,
    sample_size: u32,
    sample_flags: u16,
    sample_composition_time_offset: i32,
}

impl MergedTrackRunBoxEntryBuilder {
    pub fn new() -> MergedTrackRunBoxEntryBuilder {
        MergedTrackRunBoxEntryBuilder {
            sample_duration: 0,
            sample_size: 0,
            sample_flags: 0,
            sample_composition_time_offset: 0,
        }
    }

    pub fn with_sample_duration(mut self, sample_duration: u32) -> MergedTrackRunBoxEntryBuilder {
        self.sample_duration = sample_duration;
        self
    }

    pub fn with_sample_size(mut self, sample_size: u32) -> MergedTrackRunBoxEntryBuilder {
        self.sample_size = sample_size;
        self
    }

    pub fn with_sample_flags(mut self, sample_flags: u16) -> MergedTrackRunBoxEntryBuilder {
        self.sample_flags = sample_flags;
        self
    }

    pub fn with_sample_composition_time_offset(mut self, sample_composition_time_offset: i32) -> MergedTrackRunBoxEntryBuilder {
        self.sample_composition_time_offset = sample_composition_time_offset;
        self
    }

    pub fn build(self) -> MergedTrackRunBoxEntry {
        MergedTrackRunBoxEntry::new(self.sample_duration, self.sample_size, self.sample_flags, self.sample_composition_time_offset)
    }
}

pub struct MergedTrackRunBox {
    pub size: u32,
    pub box_type: [char; 4],
    pub version: u8,
    pub flags: U24,

    pub sample_count: u32,
    pub data_offset: u32,

    pub entries: Vec<MergedTrackRunBoxEntry>,
}

impl MergedTrackRunBox {
    pub fn new() -> MergedTrackRunBox {
        MergedTrackRunBox {
            size: 0,
            box_type: ['t', 'r', 'u', 'n'],
            version: 0,
            flags: U24::from(0x000F01),

            sample_count: 0,
            data_offset: 0,

            entries: Vec::new(),
        }
    }

    pub fn deferred_set_data_offset(mut self, data_offset: u32) -> MergedTrackRunBox {
        self.data_offset = data_offset;
        self
    }
}

impl ISerializable for MergedTrackRunBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        self.sample_count = self.entries.len() as u32;

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));
        result.push(self.version);
        result.extend_from_slice(&self.flags.serialize());
        result.extend_from_slice(&self.sample_count.to_be_bytes());
        result.extend_from_slice(&self.data_offset.to_be_bytes());
        for entry in self.entries.iter_mut() {
            result.extend_from_slice(&entry.serialize());
        }
        assert_ne!(result.len(), 0);
        result
    }

    fn size(&self) -> u32 {
        let mut size: u32 = 20;
        for entry in self.entries.iter() {
            size += entry.size();
        }
        size
    }
}

pub struct MergedTrackFragmentBox {
    pub size: u32,
    pub box_type: [char; 4],

    pub track_fragment_header_box: TrackFragmentHeaderBox,
    pub track_fragment_decode_time_box: TrackFragmentDecodeTimeBox,
    pub merged_sample_table_box: MergedSampleDependencyTableBox,
    pub merged_track_run_box: MergedTrackRunBox,
}

impl ITrackFragmentBoxLike for MergedTrackFragmentBox {
    fn deferred_set_data_offset(&mut self, data_offset: u32) {
        self.merged_track_run_box.data_offset = data_offset;
    }
}

impl ISerializable for MergedTrackFragmentBox {
    fn serialize(&mut self) -> Vec<u8> {
        self.size = self.size();

        let mut result: Vec<u8> = Vec::new();
        result.extend_from_slice(&self.size.to_be_bytes());
        result.extend_from_slice(&self.box_type.map(|c| c as u8));
        result.extend_from_slice(&self.track_fragment_header_box.serialize());
        result.extend_from_slice(&self.track_fragment_decode_time_box.serialize());
        result.extend_from_slice(&self.merged_sample_table_box.serialize());
        result.extend_from_slice(&self.merged_track_run_box.serialize());
        assert_ne!(result.len(), 0);
        result
    }

    fn size(&self) -> u32 {
        8 +
        self.track_fragment_header_box.size() +
        self.track_fragment_decode_time_box.size() +
        self.merged_sample_table_box.size() +
        self.merged_track_run_box.size()
    }
}

pub struct MergedTrackFragmentBoxBuilder {
    pub track_fragment_header_box: TrackFragmentHeaderBox,
    pub track_fragment_decode_time_box: TrackFragmentDecodeTimeBox,
    pub merged_sample_table_box: MergedSampleDependencyTableBox,
    pub merged_track_run_box: MergedTrackRunBox,
}

impl MergedTrackFragmentBoxBuilder {
    pub fn new() -> MergedTrackFragmentBoxBuilder {
        MergedTrackFragmentBoxBuilder {
            track_fragment_header_box: TrackFragmentHeaderBox::new(0),
            track_fragment_decode_time_box: TrackFragmentDecodeTimeBox::new(0),
            merged_sample_table_box: MergedSampleDependencyTableBox::new(),
            merged_track_run_box: MergedTrackRunBox::new(),
        }
    }

    pub fn with_track_id(mut self, track_id: u32) -> MergedTrackFragmentBoxBuilder {
        self.track_fragment_header_box = TrackFragmentHeaderBox::new(track_id);
        self
    }

    pub fn with_track_fragment_decode_time(mut self, decode_time: u32) -> MergedTrackFragmentBoxBuilder {
        self.track_fragment_decode_time_box = TrackFragmentDecodeTimeBox::new(decode_time);
        self
    }

    pub fn with_merged_sample_table(mut self, merged_sample_table: MergedSampleDependencyTableBox) -> MergedTrackFragmentBoxBuilder {
        self.merged_sample_table_box = merged_sample_table;
        self
    }

    pub fn with_merged_track_run(mut self, merged_track_run: MergedTrackRunBox) -> MergedTrackFragmentBoxBuilder {
        self.merged_track_run_box = merged_track_run;
        self
    }

    pub fn build(self) -> MergedTrackFragmentBox {
        MergedTrackFragmentBox {
            size: 0,
            box_type: ['t', 'r', 'a', 'f'],
            track_fragment_header_box: self.track_fragment_header_box,
            track_fragment_decode_time_box: self.track_fragment_decode_time_box,
            merged_sample_table_box: self.merged_sample_table_box,
            merged_track_run_box: self.merged_track_run_box,
        }
    }
}