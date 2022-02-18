use std::hash::Hash;
use std::path::Path;
use std::sync::RwLock;

use symphonia::core::codecs::CodecParameters;

use crate::core::analyzer::{Analyzer, PreviewSample, PREVIEW_SAMPLES_PER_PACKET};

//------------------------------------------------------------------//
//                              Track                               //
//------------------------------------------------------------------//

#[derive(Debug)]
pub struct Track {
    /// track meta data
    pub meta: TrackMeta,
    /// the file path
    pub file_path: String,
    /// the file name
    pub file_name: String,
    /// codec parameters
    pub codec_params: CodecParameters,
    /// downsampled version of decoded frames for preview
    preview_buffer: RwLock<Vec<PreviewSample>>,
    /// number of samples per packet
    /// This is used to compute the progress of the analysis
    estimated_samples_per_packet: RwLock<Option<usize>>,
    /// marks the track as analyzed
    analyzed: bool,
}

impl Track {
    pub fn new(file_path: String, codec_params: CodecParameters) -> Self {
        let file_name = String::from(Path::new(&file_path).file_name().unwrap().to_str().unwrap());
        Self {
            meta: TrackMeta::default(),
            preview_buffer: RwLock::new(vec![]),
            file_path,
            file_name,
            codec_params,
            estimated_samples_per_packet: RwLock::new(None),
            analyzed: false,
        }
    }

    /// Sets the estimated samples per packet for the track.
    /// This is needed for the progress computation, when the codec parameters don't contain this
    /// information.
    ///
    /// ATTENTION: This is a weird hack, if there is a better solution, use that!
    pub fn set_estimated_samples_per_packet(&self, estimated_samples_per_packet: usize) {
        let current_estimated_samples_per_packet =
            self.estimated_samples_per_packet.read().unwrap().clone();
        if let None = current_estimated_samples_per_packet {
            let res = Some(estimated_samples_per_packet);
            *self.estimated_samples_per_packet.write().unwrap() = res;
        }
    }

    /// append preview samples to preview buffer
    pub fn append_preview_samples(&self, preview_samples: &mut Vec<PreviewSample>) {
        // Hack: this sets the frames per packet
        // if self.avg_frames_per_packet == None {
        //     self.avg_frames_per_packet = Some((samples.len() / 2) as u64);
        // }
        self.preview_buffer.write().unwrap().append(preview_samples);
    }

    /// returns the analysis progress for this track.
    /// The result is a number between 0 and 100 (%).
    pub fn progress(&self) -> Option<u8> {
        if self.analyzed {
            Some(100)
        } else {
            let mut res = None;
            // if codec params contains max_frames_per_packet use that
            // else if estimated_samples_per_packet is set use that
            // else default to 0
            let frames_per_packet = self.get_frames_per_packet();
            // when max_frames_per_packet and number of total frames in the track are known we can
            // compute the progress
            if let (Some(max_frames_per_packet), Some(n_frames)) =
                (frames_per_packet, self.codec_params.n_frames)
            {
                let n_analyzed_packets =
                    self.preview_buffer.read().unwrap().len() / PREVIEW_SAMPLES_PER_PACKET;
                let n_analyzed_frames = n_analyzed_packets as u64 * max_frames_per_packet;
                // std::thread::sleep(Duration::from_millis(100));
                // println!("{}/{}", n_analyzed_packets, n_frames);
                res = Some((n_analyzed_frames as f64 / n_frames as f64 * 100.0).ceil() as u8);
            }
            res
        }
    }

    /// computes the number of frame per packet for this track
    fn get_frames_per_packet(&self) -> Option<u64> {
        let estimated_samples_per_packet =
            self.estimated_samples_per_packet.read().unwrap().clone();
        let frames_per_packet = self
            .codec_params
            .max_frames_per_packet
            .or(estimated_samples_per_packet.map(|x| x as u64));
        frames_per_packet.map(|x| x / 2)
    }

    /// computes the number of packets for this track
    pub fn n_packets(&self) -> Option<u64> {
        let n_frames = self.codec_params.n_frames.unwrap();
        let frames_per_packet = self.get_frames_per_packet();
        let n_packets = frames_per_packet.map(|fpp| n_frames / fpp);
        n_packets
    }

    /// returns the preview samples for a given player position and target screen size
    /// the playhead position shifts the player position by [-target_size/2, target_size/2] relative in the buffer
    pub fn live_preview(
        &self,
        target_size: usize,
        player_position: usize,
        playhead_position: usize,
    ) -> Vec<PreviewSample> {
        let preview_buffer = self.preview_buffer.read().unwrap().to_owned();
        // println!("{}", preview_buffer.len());
        let player_pos = player_position * PREVIEW_SAMPLES_PER_PACKET;
        // check if enough sampes exist for target resolution
        let diff = player_pos as isize - (target_size / 2) as isize;
        if diff >= 0 {
            // if yes return buffer content
            let l = (player_pos as f32 - (target_size as f32 / 2.0)) as usize;
            let r = (player_pos as f32 + (target_size as f32 / 2.0)) as usize;
            let r = std::cmp::min(r, preview_buffer.len());
            preview_buffer[l..r].to_owned()
        } else {
            let diff = diff.abs() as usize;
            let mut padding: Vec<PreviewSample> = vec![0.0 as f32; diff]
                .into_iter()
                .map(|s| PreviewSample {
                    mids: s,
                    lows: s,
                    highs: s,
                })
                .collect();
            if preview_buffer.len() > 0 {
                padding.extend(preview_buffer[0..target_size - diff].to_vec());
            };
            padding.to_owned()
        }
    }

    /// computes a downsampled version of the full track that fits in a buffer of target_size
    pub fn preview(&self, target_size: usize) -> Vec<PreviewSample> {
        let n_frames = self.codec_params.n_frames.unwrap();
        let frames_per_packet = self.get_frames_per_packet();
        if let Some(frames_per_packet) = frames_per_packet {
            let preview_buffer = self.preview_buffer.read().unwrap().clone();
            let n_analyzed_packets = preview_buffer.len() / PREVIEW_SAMPLES_PER_PACKET;
            let n_analyzed_frames = n_analyzed_packets as u64 * frames_per_packet;
            let progress = n_analyzed_frames as f64 / n_frames as f64 * 2.0;
            let target_size = (target_size as f64 * progress).floor() as usize;
            if target_size > 0 {
                let num_channles = self.codec_params.channels.unwrap().count();
                // let preview_buffer =
                //     Analyzer::downsample_to_preview(&preview_buffer, num_channles, target_size);
                return preview_buffer;
            }
        }
        // vec![0.0]
        (*self.preview_buffer.read().unwrap()).to_owned()
    }
}

impl Eq for Track {}

impl PartialOrd for Track {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.file_path.partial_cmp(&other.file_path)
    }
}
impl PartialEq for Track {
    fn eq(&self, other: &Self) -> bool {
        self.file_path == other.file_path
    }
}

impl Ord for Track {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.file_path.cmp(&other.file_path)
    }
}

impl Hash for Track {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.file_path.hash(state)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TrackMeta {}
impl Default for TrackMeta {
    fn default() -> Self {
        Self {}
    }
}
