mod boundaries;
mod savgol;
mod sentences;
mod similarity;

pub(super) use boundaries::{
    clamp_odd_window, filter_split_indices, find_local_minima_interpolated,
};
pub(super) use savgol::savgol_filter;
pub(super) use sentences::{group_sentences_at_boundaries, split_sentences};
pub(super) use similarity::windowed_cross_similarity;
