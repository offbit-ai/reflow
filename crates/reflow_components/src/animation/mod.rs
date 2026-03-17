//! Skeleton animation actors.
//!
//! Provides bone hierarchies, animation clips, skinning, and playback:
//!
//! - [`SkeletonActor`] — defines bone hierarchy + inverse bind matrices
//! - [`AnimationClipActor`] — keyframed animation tracks per bone
//! - [`SkinBindActor`] — associates per-vertex bone weights with a mesh
//! - [`AnimationSamplerActor`] — samples a clip at time t, outputs bone pose
//! - [`SkinningActor`] — CPU linear blend skinning (deforms mesh)
//! - [`AnimationTimeActor`] — stateful time accumulator for playback
//! - [`AnimationMixerActor`] — blends two poses

pub mod math_helpers;

mod skeleton;
mod animation_clip;
mod skin_bind;
mod animation_sampler;
mod skinning;
mod animation_time;
mod animation_mixer;
mod animation_timeline;
mod keyframe;

pub use skeleton::SkeletonActor;
pub use animation_clip::AnimationClipActor;
pub use skin_bind::SkinBindActor;
pub use animation_sampler::AnimationSamplerActor;
pub use skinning::SkinningActor;
pub use animation_time::AnimationTimeActor;
pub use animation_mixer::AnimationMixerActor;
pub use animation_timeline::AnimationTimelineActor;
pub use keyframe::KeyframeActor;
