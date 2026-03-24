//! Zeal Template to Actor Registry
//!
//! Maps Zeal template IDs to their corresponding native actor implementations.
//! Script templates (tpl_javascript_script, tpl_python_script, etc.) are not
//! mapped here — they are resolved via ComponentSpec::Script and deployed to dynASB.

use crate::Actor;
use std::collections::HashMap;
use std::sync::Arc;

use crate::animation::{
    AnimationBlendTreeActor, AnimationClipActor, AnimationEventActor, AnimationFsmActor,
    AnimationLayerActor, AnimationMixerActor, AnimationSamplerActor, AnimationTimeActor,
    AnimationTimelineActor, CharacterControllerActor, IKSolverActor, KeyframeActor,
    MorphTargetActor, RootMotionActor, SkeletonActor, SkinBindActor, SkinningActor,
    SpriteAnimationActor,
};
use crate::assets::{AssetLoadActor, AssetQueryActor, AssetStoreActor};
use crate::flow_control::{
    CollectActor, ConditionalBranchActor, CronTriggerActor, DelayActor, FilterActor, FsmActor,
    GateActor, HitTestActor, IntervalTriggerActor, LoopActor, MapActor, MergeActor,
    PassthroughActor, ReduceActor, ServerRequestActor, ServerResponseActor, SignalActor,
    SplitActor, SubscriberActor, SwitchCaseActor,
};
use crate::gpu::font_load::FontLoadActor;
use crate::gpu::glyph_atlas::GlyphAtlasActor;
use crate::gpu::shader::{
    ShaderBrickTextureActor, ShaderBumpMapActor, ShaderCheckerTextureActor,
    ShaderClampActor, ShaderColorMixActor, ShaderColorRampActor, ShaderCombineXYZActor,
    ShaderCompilerActor, ShaderConstColorActor, ShaderConstFloatActor, ShaderFresnelActor,
    ShaderGradientTextureActor, ShaderImageTextureActor, ShaderMapRangeActor,
    ShaderMappingActor, ShaderMaterialOutputActor, ShaderMathActor,
    ShaderMusgraveTextureActor, ShaderNoiseTextureActor, ShaderNormalInputActor,
    ShaderNormalMapActor, ShaderPositionInputActor, ShaderPrincipledBsdfActor,
    ShaderSeparateXYZActor, ShaderTexCoordActor, ShaderTimeInputActor,
    ShaderVertexColorActor, ShaderVoronoiTextureActor, ShaderWaveTextureActor,
};
#[cfg(feature = "gpu")]
use crate::gpu::scene_render::SceneRenderActor;
use crate::gpu::sdf::SdfPathActor;
#[cfg(feature = "gpu")]
use crate::gpu::sdf::{MeshToSdfActor, SdfLiveRenderActor, SdfMarchingCubesActor, SdfRenderActor};
use crate::gpu::sdf::{
    SdfBendActor, SdfBoxActor, SdfCapsuleActor, SdfConeActor, SdfCylinderActor, SdfDifferenceActor,
    SdfDisplaceActor, SdfIntersectionActor, SdfMaterialActor, SdfMirrorActor, SdfPlaneActor,
    SdfRepeatActor, SdfRotateActor, SdfRoundActor, SdfScaleActor, SdfSceneActor, SdfShellActor,
    SdfSmoothDifferenceActor, SdfSmoothIntersectionActor, SdfSmoothUnionActor, SdfSphereActor,
    SdfTorusActor, SdfTranslateActor, SdfTwistActor, SdfUnionActor,
};
#[cfg(feature = "gpu")]
use crate::gpu::sdf_2d::Gpu2DRenderActor;
#[cfg(feature = "window-events")]
use crate::input::{
    GamepadInputActor, KeyboardInputActor, MouseInputActor, TouchInputActor, WindowEventActor,
};
use crate::integration::HttpRequestActor;
#[cfg(feature = "browser")]
use crate::integration::BrowserScreencastActor;
use crate::io::{
    FbxImportActor, FileLoadActor, FileSaveActor, GltfExportActor, GltfImportActor,
    MeshImportActor, ObjExportActor, ObjImportActor, SceneImportActor, StlExportActor,
    StlImportActor,
};
use crate::logic::RulesEngineActor;
use crate::math::{
    Mat4IdentityActor,
    Mat4LookAtActor,
    // Matrix
    Mat4MultiplyActor,
    Mat4PerspectiveActor,
    Mat4RotateXActor,
    Mat4RotateYActor,
    Mat4RotateZActor,
    Mat4ScaleActor,
    Mat4TransformActor,
    Mat4TranslateActor,
    MathAbsoluteActor,
    MathAddActor,
    MathAverageActor,
    MathClampActor,
    MathDivideActor,
    MathExpressionActor,
    MathMinMaxActor,
    MathModuloActor,
    MathMultiplyActor,
    MathPowerActor,
    MathRandomActor,
    MathRoundActor,
    MathSqrtActor,
    MathStatisticsActor,
    MathSubtractActor,
    MathSumActor,
    // Quaternion
    QuatFromEulerActor,
    QuatMultiplyActor,
    QuatRotateVec3Actor,
    QuatSlerpActor,
    // Vector
    Vec3Actor,
    Vec3AddActor,
    Vec3CrossActor,
    Vec3DistanceActor,
    Vec3DotActor,
    Vec3LengthActor,
    Vec3LerpActor,
    Vec3NormalizeActor,
    Vec3ReflectActor,
    Vec3ScaleActor,
    Vec3SubtractActor,
};
use crate::media::{
    AudioInputActor, AudioStreamDisplayActor, ImageInputActor, ImageStreamDisplayActor,
    VideoInputActor,
};
use crate::procedural::{
    HeightmapToImageActor, HeightmapToMeshActor, ImageToHeightmapActor, LSystemActor,
    MeshCombineActor, NoiseGeneratorActor, ParticleEmitterActor, TriplanarTextureActor,
    TubeMeshActor, UVTextureActor, VertexColorActor, VoronoiActor,
};
use crate::scene::{ComponentNodeActor, InstanceActor, PrefabActor, SceneGraphActor, TerrainActor};
#[cfg(feature = "video-encode")]
use crate::stream_ops::VideoEncoderActor;
use crate::stream_ops::{
    AudioGainActor, AudioNormalizeActor, AudioSpectrumActor, BiquadFilterActor,
    BrightnessContrastActor, BytesToStreamActor, ChromaKeyActor, CompressorActor, ConvolveActor,
    CorrelatorActor, CrossoverActor, DCOffsetActor, DeEsserActor, EnvelopeFollowerActor,
    EqualizerActor, GrayscaleFilterActor, IFFTActor, ImageDecodeActor, ImageEncodeActor,
    ImageResizeActor, LimiterActor, NoiseGateActor, NoiseReductionActor, PeakDetectActor,
    PitchShiftActor, RenderFrameCollectorActor, SilenceDetectActor, StreamBufferActor,
    StreamStatsActor, StreamTeeActor, StreamThrottleActor, StreamToBytesActor, TimeStretchActor,
};
use crate::systems::{
    BehaviorSystemActor, LayoutSyncSystemActor, SceneBillboardSystemActor, SceneCameraSystemActor,
    SceneLightCollectorActor, SceneMaterialSystemActor, ScenePhysicsSystemActor,
    SceneSkyboxSystemActor, SceneWeatherSystemActor, StateMachineSystemActor,
    TextRenderSystemActor, TextSdfSystemActor, TimelineSystemActor, TweenSystemActor,
};
use crate::text::{DateTimeActor, JsonParserActor, RegexMatcherActor};
use crate::transform::{DataEmitActor, DataOperationsActor, DataTransformActor, GeneratorActor};
use crate::vector::{
    BackgroundActor, BlendModeActor, Canvas2DActor, GaussianBlurActor, Shape2DActor,
    VectorRasterizeActor,
};

/// Get an actor instance for a given Zeal template ID
pub fn get_actor_for_template(template_id: &str) -> Option<Arc<dyn Actor>> {
    match template_id {
        // Asset DB
        "tpl_asset_store" => Some(Arc::new(AssetStoreActor::new())),
        "tpl_asset_load" => Some(Arc::new(AssetLoadActor::new())),
        "tpl_asset_query" => Some(Arc::new(AssetQueryActor::new())),

        // Scene Systems (ECS — read/write AssetDB components)
        "tpl_scene_physics" => Some(Arc::new(ScenePhysicsSystemActor::new())),
        "tpl_scene_camera" => Some(Arc::new(SceneCameraSystemActor::new())),
        "tpl_scene_light_collector" => Some(Arc::new(SceneLightCollectorActor::new())),
        "tpl_scene_material" => Some(Arc::new(SceneMaterialSystemActor::new())),
        "tpl_scene_billboard" => Some(Arc::new(SceneBillboardSystemActor::new())),
        "tpl_scene_skybox" => Some(Arc::new(SceneSkyboxSystemActor::new())),
        "tpl_scene_weather" => Some(Arc::new(SceneWeatherSystemActor::new())),

        // Universal Systems (motion design, interactive animation, design engineering)
        "tpl_tween_system" => Some(Arc::new(TweenSystemActor::new())),
        "tpl_timeline_system" => Some(Arc::new(TimelineSystemActor::new())),
        "tpl_state_machine_system" => Some(Arc::new(StateMachineSystemActor::new())),
        "tpl_behavior_system" => Some(Arc::new(BehaviorSystemActor::new())),
        "tpl_layout_sync" => Some(Arc::new(LayoutSyncSystemActor::new())),
        "tpl_text_render" => Some(Arc::new(TextRenderSystemActor::new())),
        "tpl_text_sdf" => Some(Arc::new(TextSdfSystemActor::new())),

        // Integration
        "tpl_http_request" => Some(Arc::new(HttpRequestActor::new())),
        #[cfg(feature = "browser")]
        "tpl_browser_screencast" => Some(Arc::new(BrowserScreencastActor::new())),

        // Flow Control
        "tpl_fsm" => Some(Arc::new(FsmActor::new())),
        "tpl_hit_test" => Some(Arc::new(HitTestActor::new())),
        "tpl_signal" => Some(Arc::new(SignalActor::new())),
        "tpl_subscriber" => Some(Arc::new(SubscriberActor::new())),
        "tpl_if_branch" => Some(Arc::new(ConditionalBranchActor::new())),
        "tpl_switch" => Some(Arc::new(SwitchCaseActor::new())),
        "tpl_loop" => Some(Arc::new(LoopActor::new())),

        // Scene
        "tpl_component" => Some(Arc::new(ComponentNodeActor::new())),
        "tpl_prefab" => Some(Arc::new(PrefabActor::new())),
        "tpl_instance" => Some(Arc::new(InstanceActor::new())),
        "tpl_scene_graph" => Some(Arc::new(SceneGraphActor::new())),
        "tpl_terrain" => Some(Arc::new(TerrainActor::new())),

        // Input Events (feature-gated)
        #[cfg(feature = "window-events")]
        "tpl_keyboard_input" => Some(Arc::new(KeyboardInputActor::new())),
        #[cfg(feature = "window-events")]
        "tpl_mouse_input" => Some(Arc::new(MouseInputActor::new())),
        #[cfg(feature = "window-events")]
        "tpl_gamepad_input" => Some(Arc::new(GamepadInputActor::new())),
        #[cfg(feature = "window-events")]
        "tpl_touch_input" => Some(Arc::new(TouchInputActor::new())),
        #[cfg(feature = "window-events")]
        "tpl_window_event" => Some(Arc::new(WindowEventActor::new())),

        // Triggers
        "tpl_interval_trigger" => Some(Arc::new(IntervalTriggerActor::new())),
        "tpl_cron_trigger" => Some(Arc::new(CronTriggerActor::new())),

        // Server
        "tpl_server_request" => Some(Arc::new(ServerRequestActor::new())),
        "tpl_server_response" => Some(Arc::new(ServerResponseActor::new())),

        // Flow Utilities
        "tpl_map" => Some(Arc::new(MapActor::new())),
        "tpl_filter" => Some(Arc::new(FilterActor::new())),
        "tpl_reduce" => Some(Arc::new(ReduceActor::new())),
        "tpl_merge" => Some(Arc::new(MergeActor::new())),
        "tpl_split" => Some(Arc::new(SplitActor::new())),
        "tpl_delay" => Some(Arc::new(DelayActor::new())),
        "tpl_gate" => Some(Arc::new(GateActor::new())),
        "tpl_collect" => Some(Arc::new(CollectActor::new())),
        "tpl_passthrough" => Some(Arc::new(PassthroughActor::new())),

        // Data Processing
        "tpl_data_emit" => Some(Arc::new(DataEmitActor::new())),
        "tpl_data_transformer" => Some(Arc::new(DataTransformActor::new())),
        "tpl_data_operations" => Some(Arc::new(DataOperationsActor::new())),
        "tpl_generator" => Some(Arc::new(GeneratorActor::new())),

        // Logic
        "tpl_rules_engine" => Some(Arc::new(RulesEngineActor::new())),

        // Media
        "tpl_image_input" => Some(Arc::new(ImageInputActor::new())),
        "tpl_audio_input" => Some(Arc::new(AudioInputActor::new())),
        "tpl_video_input" => Some(Arc::new(VideoInputActor::new())),

        // Math
        "tpl_math_add" => Some(Arc::new(MathAddActor::new())),
        "tpl_math_subtract" => Some(Arc::new(MathSubtractActor::new())),
        "tpl_math_multiply" => Some(Arc::new(MathMultiplyActor::new())),
        "tpl_math_divide" => Some(Arc::new(MathDivideActor::new())),
        "tpl_math_modulo" => Some(Arc::new(MathModuloActor::new())),
        "tpl_math_power" => Some(Arc::new(MathPowerActor::new())),
        "tpl_math_sqrt" => Some(Arc::new(MathSqrtActor::new())),
        "tpl_math_absolute" => Some(Arc::new(MathAbsoluteActor::new())),
        "tpl_math_clamp" => Some(Arc::new(MathClampActor::new())),
        "tpl_math_min_max" => Some(Arc::new(MathMinMaxActor::new())),
        "tpl_math_round" => Some(Arc::new(MathRoundActor::new())),
        "tpl_math_random" => Some(Arc::new(MathRandomActor::new())),
        "tpl_math_average" => Some(Arc::new(MathAverageActor::new())),
        "tpl_math_sum" => Some(Arc::new(MathSumActor::new())),
        "tpl_math_statistics" => Some(Arc::new(MathStatisticsActor::new())),
        "tpl_math_expression" => Some(Arc::new(MathExpressionActor::new())),

        // Vector3
        "tpl_vec3" => Some(Arc::new(Vec3Actor::new())),
        "tpl_vec3_add" => Some(Arc::new(Vec3AddActor::new())),
        "tpl_vec3_subtract" => Some(Arc::new(Vec3SubtractActor::new())),
        "tpl_vec3_scale" => Some(Arc::new(Vec3ScaleActor::new())),
        "tpl_vec3_dot" => Some(Arc::new(Vec3DotActor::new())),
        "tpl_vec3_cross" => Some(Arc::new(Vec3CrossActor::new())),
        "tpl_vec3_normalize" => Some(Arc::new(Vec3NormalizeActor::new())),
        "tpl_vec3_length" => Some(Arc::new(Vec3LengthActor::new())),
        "tpl_vec3_distance" => Some(Arc::new(Vec3DistanceActor::new())),
        "tpl_vec3_lerp" => Some(Arc::new(Vec3LerpActor::new())),
        "tpl_vec3_reflect" => Some(Arc::new(Vec3ReflectActor::new())),

        // Matrix4
        "tpl_mat4_identity" => Some(Arc::new(Mat4IdentityActor::new())),
        "tpl_mat4_multiply" => Some(Arc::new(Mat4MultiplyActor::new())),
        "tpl_mat4_transform" => Some(Arc::new(Mat4TransformActor::new())),
        "tpl_mat4_translate" => Some(Arc::new(Mat4TranslateActor::new())),
        "tpl_mat4_scale" => Some(Arc::new(Mat4ScaleActor::new())),
        "tpl_mat4_rotate_x" => Some(Arc::new(Mat4RotateXActor::new())),
        "tpl_mat4_rotate_y" => Some(Arc::new(Mat4RotateYActor::new())),
        "tpl_mat4_rotate_z" => Some(Arc::new(Mat4RotateZActor::new())),
        "tpl_mat4_look_at" => Some(Arc::new(Mat4LookAtActor::new())),
        "tpl_mat4_perspective" => Some(Arc::new(Mat4PerspectiveActor::new())),

        // Quaternion
        "tpl_quat_from_euler" => Some(Arc::new(QuatFromEulerActor::new())),
        "tpl_quat_multiply" => Some(Arc::new(QuatMultiplyActor::new())),
        "tpl_quat_slerp" => Some(Arc::new(QuatSlerpActor::new())),
        "tpl_quat_rotate_vec3" => Some(Arc::new(QuatRotateVec3Actor::new())),

        // Procedural
        "tpl_noise_generator" => Some(Arc::new(NoiseGeneratorActor::new())),

        // Procedural / Heightmap
        "tpl_image_to_heightmap" => Some(Arc::new(ImageToHeightmapActor::new())),
        "tpl_heightmap_to_image" => Some(Arc::new(HeightmapToImageActor::new())),
        "tpl_heightmap_to_mesh" => Some(Arc::new(HeightmapToMeshActor::new())),
        "tpl_voronoi" => Some(Arc::new(VoronoiActor::new())),
        "tpl_lsystem" => Some(Arc::new(LSystemActor::new())),
        "tpl_particle_emitter" => Some(Arc::new(ParticleEmitterActor::new())),
        "tpl_triplanar_texture" => Some(Arc::new(TriplanarTextureActor::new())),
        "tpl_mesh_combine" => Some(Arc::new(MeshCombineActor::new())),
        "tpl_tube_mesh" => Some(Arc::new(TubeMeshActor::new())),
        "tpl_vertex_color" => Some(Arc::new(VertexColorActor::new())),
        "tpl_uv_texture" => Some(Arc::new(UVTextureActor::new())),

        // Text / Utilities
        "tpl_json_parser" => Some(Arc::new(JsonParserActor::new())),
        "tpl_regex_matcher" => Some(Arc::new(RegexMatcherActor::new())),
        "tpl_date_time" => Some(Arc::new(DateTimeActor::new())),

        // Image Codecs
        "tpl_image_decode" => Some(Arc::new(ImageDecodeActor::new())),
        "tpl_image_encode" => Some(Arc::new(ImageEncodeActor::new())),

        // File I/O
        "tpl_file_load" => Some(Arc::new(FileLoadActor::new())),
        "tpl_file_save" => Some(Arc::new(FileSaveActor::new())),

        // Stream Display
        "tpl_image_stream_display" => Some(Arc::new(ImageStreamDisplayActor::new())),
        "tpl_audio_stream_display" => Some(Arc::new(AudioStreamDisplayActor::new())),

        // Stream Operations
        "tpl_bytes_to_stream" => Some(Arc::new(BytesToStreamActor::new())),
        "tpl_stream_to_bytes" => Some(Arc::new(StreamToBytesActor::new())),
        "tpl_stream_tee" => Some(Arc::new(StreamTeeActor::new())),
        "tpl_stream_buffer" => Some(Arc::new(StreamBufferActor::new())),
        "tpl_stream_throttle" => Some(Arc::new(StreamThrottleActor::new())),
        "tpl_stream_stats" => Some(Arc::new(StreamStatsActor::new())),

        // Image DSP
        "tpl_grayscale_filter" => Some(Arc::new(GrayscaleFilterActor::new())),
        "tpl_brightness_contrast" => Some(Arc::new(BrightnessContrastActor::new())),
        "tpl_chroma_key" => Some(Arc::new(ChromaKeyActor::new())),

        // Audio DSP
        "tpl_audio_gain" => Some(Arc::new(AudioGainActor::new())),
        "tpl_biquad_filter" => Some(Arc::new(BiquadFilterActor::new())),
        "tpl_compressor" => Some(Arc::new(CompressorActor::new())),
        "tpl_audio_normalize" => Some(Arc::new(AudioNormalizeActor::new())),
        "tpl_noise_gate" => Some(Arc::new(NoiseGateActor::new())),
        "tpl_de_esser" => Some(Arc::new(DeEsserActor::new())),
        "tpl_audio_spectrum" => Some(Arc::new(AudioSpectrumActor::new())),
        "tpl_silence_detect" => Some(Arc::new(SilenceDetectActor::new())),

        // Audio DSP (continued)
        "tpl_equalizer" => Some(Arc::new(EqualizerActor::new())),
        "tpl_limiter" => Some(Arc::new(LimiterActor::new())),
        "tpl_dc_offset" => Some(Arc::new(DCOffsetActor::new())),
        "tpl_envelope_follower" => Some(Arc::new(EnvelopeFollowerActor::new())),
        "tpl_crossover" => Some(Arc::new(CrossoverActor::new())),
        "tpl_peak_detect" => Some(Arc::new(PeakDetectActor::new())),
        "tpl_ifft" => Some(Arc::new(IFFTActor::new())),
        "tpl_convolve" => Some(Arc::new(ConvolveActor::new())),
        "tpl_noise_reduction" => Some(Arc::new(NoiseReductionActor::new())),
        "tpl_pitch_shift" => Some(Arc::new(PitchShiftActor::new())),
        "tpl_time_stretch" => Some(Arc::new(TimeStretchActor::new())),
        "tpl_correlator" => Some(Arc::new(CorrelatorActor::new())),

        // Image DSP (continued)
        "tpl_image_resize" => Some(Arc::new(ImageResizeActor::new())),

        // SDF (always available — pure IR composition)
        "tpl_sdf_sphere" => Some(Arc::new(SdfSphereActor::new())),
        "tpl_sdf_box" => Some(Arc::new(SdfBoxActor::new())),
        "tpl_sdf_cylinder" => Some(Arc::new(SdfCylinderActor::new())),
        "tpl_sdf_torus" => Some(Arc::new(SdfTorusActor::new())),
        "tpl_sdf_capsule" => Some(Arc::new(SdfCapsuleActor::new())),
        "tpl_sdf_cone" => Some(Arc::new(SdfConeActor::new())),
        "tpl_sdf_plane" => Some(Arc::new(SdfPlaneActor::new())),
        "tpl_sdf_union" => Some(Arc::new(SdfUnionActor::new())),
        "tpl_sdf_intersection" => Some(Arc::new(SdfIntersectionActor::new())),
        "tpl_sdf_difference" => Some(Arc::new(SdfDifferenceActor::new())),
        "tpl_sdf_smooth_union" => Some(Arc::new(SdfSmoothUnionActor::new())),
        "tpl_sdf_smooth_intersection" => Some(Arc::new(SdfSmoothIntersectionActor::new())),
        "tpl_sdf_smooth_difference" => Some(Arc::new(SdfSmoothDifferenceActor::new())),
        "tpl_sdf_translate" => Some(Arc::new(SdfTranslateActor::new())),
        "tpl_sdf_rotate" => Some(Arc::new(SdfRotateActor::new())),
        "tpl_sdf_scale" => Some(Arc::new(SdfScaleActor::new())),
        "tpl_sdf_twist" => Some(Arc::new(SdfTwistActor::new())),
        "tpl_sdf_bend" => Some(Arc::new(SdfBendActor::new())),
        "tpl_sdf_round" => Some(Arc::new(SdfRoundActor::new())),
        "tpl_sdf_shell" => Some(Arc::new(SdfShellActor::new())),
        "tpl_sdf_mirror" => Some(Arc::new(SdfMirrorActor::new())),
        "tpl_sdf_repeat" => Some(Arc::new(SdfRepeatActor::new())),
        "tpl_sdf_displace" => Some(Arc::new(SdfDisplaceActor::new())),
        "tpl_sdf_material" => Some(Arc::new(SdfMaterialActor::new())),
        "tpl_sdf_scene" => Some(Arc::new(SdfSceneActor::new())),

        // SDF path (always available — pure IR composition)
        "tpl_sdf_path" => Some(Arc::new(SdfPathActor::new())),

        // GPU compute (requires wgpu)
        #[cfg(feature = "gpu")]
        "tpl_sdf_live_render" => Some(Arc::new(SdfLiveRenderActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_render" => Some(Arc::new(SdfRenderActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_sdf_marching_cubes" => Some(Arc::new(SdfMarchingCubesActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_mesh_to_sdf" => Some(Arc::new(MeshToSdfActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_scene_render" => Some(Arc::new(SceneRenderActor::new())),
        #[cfg(feature = "gpu")]
        "tpl_gpu_2d_render" => Some(Arc::new(Gpu2DRenderActor::new())),
        "tpl_font_load" => Some(Arc::new(FontLoadActor::new())),
        "tpl_glyph_atlas" => Some(Arc::new(GlyphAtlasActor::new())),

        // Shader Graph (node-based materials)
        "tpl_shader_compiler" => Some(Arc::new(ShaderCompilerActor::new())),
        "tpl_shader_principled_bsdf" => Some(Arc::new(ShaderPrincipledBsdfActor::new())),
        "tpl_shader_material_output" => Some(Arc::new(ShaderMaterialOutputActor::new())),
        "tpl_shader_const_float" => Some(Arc::new(ShaderConstFloatActor::new())),
        "tpl_shader_const_color" => Some(Arc::new(ShaderConstColorActor::new())),
        "tpl_shader_texcoord" => Some(Arc::new(ShaderTexCoordActor::new())),
        "tpl_shader_position" => Some(Arc::new(ShaderPositionInputActor::new())),
        "tpl_shader_normal" => Some(Arc::new(ShaderNormalInputActor::new())),
        "tpl_shader_time" => Some(Arc::new(ShaderTimeInputActor::new())),
        "tpl_shader_vertex_color" => Some(Arc::new(ShaderVertexColorActor::new())),
        "tpl_shader_image_texture" => Some(Arc::new(ShaderImageTextureActor::new())),
        "tpl_shader_noise_texture" => Some(Arc::new(ShaderNoiseTextureActor::new())),
        "tpl_shader_checker_texture" => Some(Arc::new(ShaderCheckerTextureActor::new())),
        "tpl_shader_math" => Some(Arc::new(ShaderMathActor::new())),
        "tpl_shader_color_mix" => Some(Arc::new(ShaderColorMixActor::new())),
        "tpl_shader_color_ramp" => Some(Arc::new(ShaderColorRampActor::new())),
        "tpl_shader_fresnel" => Some(Arc::new(ShaderFresnelActor::new())),
        "tpl_shader_normal_map" => Some(Arc::new(ShaderNormalMapActor::new())),
        "tpl_shader_bump_map" => Some(Arc::new(ShaderBumpMapActor::new())),
        "tpl_shader_mapping" => Some(Arc::new(ShaderMappingActor::new())),
        "tpl_shader_separate_xyz" => Some(Arc::new(ShaderSeparateXYZActor::new())),
        "tpl_shader_combine_xyz" => Some(Arc::new(ShaderCombineXYZActor::new())),
        "tpl_shader_clamp" => Some(Arc::new(ShaderClampActor::new())),
        "tpl_shader_map_range" => Some(Arc::new(ShaderMapRangeActor::new())),
        "tpl_shader_voronoi_texture" => Some(Arc::new(ShaderVoronoiTextureActor::new())),
        "tpl_shader_gradient_texture" => Some(Arc::new(ShaderGradientTextureActor::new())),
        "tpl_shader_brick_texture" => Some(Arc::new(ShaderBrickTextureActor::new())),
        "tpl_shader_musgrave_texture" => Some(Arc::new(ShaderMusgraveTextureActor::new())),
        "tpl_shader_wave_texture" => Some(Arc::new(ShaderWaveTextureActor::new())),

        // Animation
        "tpl_skeleton" => Some(Arc::new(SkeletonActor::new())),
        "tpl_animation_clip" => Some(Arc::new(AnimationClipActor::new())),
        "tpl_skin_bind" => Some(Arc::new(SkinBindActor::new())),
        "tpl_animation_sampler" => Some(Arc::new(AnimationSamplerActor::new())),
        "tpl_skinning" => Some(Arc::new(SkinningActor::new())),
        "tpl_animation_time" => Some(Arc::new(AnimationTimeActor::new())),
        "tpl_animation_mixer" => Some(Arc::new(AnimationMixerActor::new())),
        "tpl_keyframe" => Some(Arc::new(KeyframeActor::new())),
        "tpl_animation_timeline" => Some(Arc::new(AnimationTimelineActor::new())),
        "tpl_sprite_animation" => Some(Arc::new(SpriteAnimationActor::new())),
        "tpl_animation_blend_tree" => Some(Arc::new(AnimationBlendTreeActor::new())),
        "tpl_animation_fsm" => Some(Arc::new(AnimationFsmActor::new())),
        "tpl_ik_solver" => Some(Arc::new(IKSolverActor::new())),
        "tpl_root_motion" => Some(Arc::new(RootMotionActor::new())),
        "tpl_animation_layer" => Some(Arc::new(AnimationLayerActor::new())),
        "tpl_morph_target" => Some(Arc::new(MorphTargetActor::new())),
        "tpl_animation_event" => Some(Arc::new(AnimationEventActor::new())),
        "tpl_character_controller" => Some(Arc::new(CharacterControllerActor::new())),

        // Video
        "tpl_frame_buffer" => Some(Arc::new(crate::stream_ops::FrameBufferActor::new())),
        "tpl_render_frame_collector" => Some(Arc::new(RenderFrameCollectorActor::new())),
        #[cfg(feature = "video-encode")]
        "tpl_video_encoder" => Some(Arc::new(VideoEncoderActor::new())),

        // Mesh export
        "tpl_obj_export" => Some(Arc::new(ObjExportActor::new())),
        "tpl_stl_export" => Some(Arc::new(StlExportActor::new())),
        "tpl_gltf_export" => Some(Arc::new(GltfExportActor::new())),

        // Model/scene import
        "tpl_stl_import" => Some(Arc::new(StlImportActor::new())),
        "tpl_obj_import" => Some(Arc::new(ObjImportActor::new())),
        "tpl_gltf_import" => Some(Arc::new(GltfImportActor::new())),
        "tpl_mesh_import" => Some(Arc::new(MeshImportActor::new())),
        "tpl_scene_import" => Some(Arc::new(SceneImportActor::new())),
        "tpl_fbx_import" => Some(Arc::new(FbxImportActor::new())),

        // 2D Vector Graphics
        "tpl_shape_2d" => Some(Arc::new(Shape2DActor::new())),
        "tpl_vector_rasterize" => Some(Arc::new(VectorRasterizeActor::new())),
        "tpl_gaussian_blur" => Some(Arc::new(GaussianBlurActor::new())),
        "tpl_blend_mode" => Some(Arc::new(BlendModeActor::new())),
        "tpl_canvas_2d" => Some(Arc::new(Canvas2DActor::new())),
        "tpl_background" => Some(Arc::new(BackgroundActor::new())),

        // Fall through to generated API actors (api_slack_send_message, etc.)
        #[cfg(feature = "api")]
        other => crate::api::api_registry::get_api_actor_for_template(other),
        #[cfg(not(feature = "api"))]
        _ => None,
    }
}

/// Get the complete mapping of template IDs to actor names
pub fn get_template_mapping() -> HashMap<String, String> {
    let mut mapping = HashMap::new();

    // Asset DB
    mapping.insert("tpl_asset_store".to_string(), "AssetStoreActor".to_string());
    mapping.insert("tpl_asset_load".to_string(), "AssetLoadActor".to_string());
    mapping.insert("tpl_asset_query".to_string(), "AssetQueryActor".to_string());

    // Scene Systems
    mapping.insert(
        "tpl_scene_physics".to_string(),
        "ScenePhysicsSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_scene_camera".to_string(),
        "SceneCameraSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_scene_light_collector".to_string(),
        "SceneLightCollectorActor".to_string(),
    );
    mapping.insert(
        "tpl_scene_material".to_string(),
        "SceneMaterialSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_scene_billboard".to_string(),
        "SceneBillboardSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_scene_skybox".to_string(),
        "SceneSkyboxSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_scene_weather".to_string(),
        "SceneWeatherSystemActor".to_string(),
    );

    // Universal Systems
    mapping.insert(
        "tpl_tween_system".to_string(),
        "TweenSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_timeline_system".to_string(),
        "TimelineSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_state_machine_system".to_string(),
        "StateMachineSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_behavior_system".to_string(),
        "BehaviorSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_layout_sync".to_string(),
        "LayoutSyncSystemActor".to_string(),
    );
    mapping.insert(
        "tpl_text_render".to_string(),
        "TextRenderSystemActor".to_string(),
    );
    mapping.insert("tpl_text_sdf".to_string(), "TextSdfSystemActor".to_string());

    mapping.insert(
        "tpl_http_request".to_string(),
        "HttpRequestActor".to_string(),
    );
    #[cfg(feature = "browser")]
    mapping.insert(
        "tpl_browser_screencast".to_string(),
        "BrowserScreencastActor".to_string(),
    );
    mapping.insert("tpl_fsm".to_string(), "FsmActor".to_string());
    mapping.insert(
        "tpl_hit_test".to_string(),
        "HitTestActor".to_string(),
    );
    mapping.insert(
        "tpl_signal".to_string(),
        "SignalActor".to_string(),
    );
    mapping.insert(
        "tpl_subscriber".to_string(),
        "SubscriberActor".to_string(),
    );
    mapping.insert(
        "tpl_if_branch".to_string(),
        "ConditionalBranchActor".to_string(),
    );
    mapping.insert("tpl_switch".to_string(), "SwitchCaseActor".to_string());
    mapping.insert("tpl_loop".to_string(), "LoopActor".to_string());
    mapping.insert(
        "tpl_data_emit".to_string(),
        "DataEmitActor".to_string(),
    );
    mapping.insert(
        "tpl_data_transformer".to_string(),
        "DataTransformActor".to_string(),
    );
    mapping.insert(
        "tpl_data_operations".to_string(),
        "DataOperationsActor".to_string(),
    );
    mapping.insert("tpl_generator".to_string(), "GeneratorActor".to_string());
    mapping.insert(
        "tpl_rules_engine".to_string(),
        "RulesEngineActor".to_string(),
    );
    mapping.insert("tpl_image_input".to_string(), "ImageInputActor".to_string());
    mapping.insert("tpl_audio_input".to_string(), "AudioInputActor".to_string());
    mapping.insert("tpl_video_input".to_string(), "VideoInputActor".to_string());
    mapping.insert(
        "tpl_image_stream_display".to_string(),
        "ImageStreamDisplayActor".to_string(),
    );
    mapping.insert(
        "tpl_audio_stream_display".to_string(),
        "AudioStreamDisplayActor".to_string(),
    );

    // Stream Operations
    mapping.insert(
        "tpl_bytes_to_stream".to_string(),
        "BytesToStreamActor".to_string(),
    );
    mapping.insert(
        "tpl_stream_to_bytes".to_string(),
        "StreamToBytesActor".to_string(),
    );
    mapping.insert("tpl_stream_tee".to_string(), "StreamTeeActor".to_string());
    mapping.insert(
        "tpl_stream_buffer".to_string(),
        "StreamBufferActor".to_string(),
    );
    mapping.insert(
        "tpl_stream_throttle".to_string(),
        "StreamThrottleActor".to_string(),
    );
    mapping.insert(
        "tpl_stream_stats".to_string(),
        "StreamStatsActor".to_string(),
    );

    // Image DSP
    mapping.insert(
        "tpl_grayscale_filter".to_string(),
        "GrayscaleFilterActor".to_string(),
    );
    mapping.insert(
        "tpl_brightness_contrast".to_string(),
        "BrightnessContrastActor".to_string(),
    );
    mapping.insert("tpl_chroma_key".to_string(), "ChromaKeyActor".to_string());

    // Audio DSP
    mapping.insert("tpl_audio_gain".to_string(), "AudioGainActor".to_string());
    mapping.insert(
        "tpl_biquad_filter".to_string(),
        "BiquadFilterActor".to_string(),
    );
    mapping.insert("tpl_compressor".to_string(), "CompressorActor".to_string());
    mapping.insert(
        "tpl_audio_normalize".to_string(),
        "AudioNormalizeActor".to_string(),
    );
    mapping.insert("tpl_noise_gate".to_string(), "NoiseGateActor".to_string());
    mapping.insert("tpl_de_esser".to_string(), "DeEsserActor".to_string());
    mapping.insert(
        "tpl_audio_spectrum".to_string(),
        "AudioSpectrumActor".to_string(),
    );
    mapping.insert(
        "tpl_silence_detect".to_string(),
        "SilenceDetectActor".to_string(),
    );

    // Image DSP (continued)
    mapping.insert(
        "tpl_image_resize".to_string(),
        "ImageResizeActor".to_string(),
    );

    // Input Events (feature-gated)
    #[cfg(feature = "window-events")]
    for (id, name) in [
        ("tpl_keyboard_input", "KeyboardInputActor"),
        ("tpl_mouse_input", "MouseInputActor"),
        ("tpl_gamepad_input", "GamepadInputActor"),
        ("tpl_touch_input", "TouchInputActor"),
        ("tpl_window_event", "WindowEventActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }

    // Scene
    for (id, name) in [
        ("tpl_component", "ComponentNodeActor"),
        ("tpl_prefab", "PrefabActor"),
        ("tpl_instance", "InstanceActor"),
        ("tpl_scene_graph", "SceneGraphActor"),
        ("tpl_terrain", "TerrainActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }

    // Triggers + Server
    mapping.insert(
        "tpl_interval_trigger".to_string(),
        "IntervalTriggerActor".to_string(),
    );
    mapping.insert(
        "tpl_cron_trigger".to_string(),
        "CronTriggerActor".to_string(),
    );
    mapping.insert(
        "tpl_server_request".to_string(),
        "ServerRequestActor".to_string(),
    );
    mapping.insert(
        "tpl_server_response".to_string(),
        "ServerResponseActor".to_string(),
    );

    // Flow Utilities
    for (id, name) in [
        ("tpl_map", "MapActor"),
        ("tpl_filter", "FilterActor"),
        ("tpl_reduce", "ReduceActor"),
        ("tpl_merge", "MergeActor"),
        ("tpl_split", "SplitActor"),
        ("tpl_delay", "DelayActor"),
        ("tpl_gate", "GateActor"),
        ("tpl_collect", "CollectActor"),
        ("tpl_passthrough", "PassthroughActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }

    // Math
    for (id, name) in [
        ("tpl_math_add", "MathAddActor"),
        ("tpl_math_subtract", "MathSubtractActor"),
        ("tpl_math_multiply", "MathMultiplyActor"),
        ("tpl_math_divide", "MathDivideActor"),
        ("tpl_math_modulo", "MathModuloActor"),
        ("tpl_math_power", "MathPowerActor"),
        ("tpl_math_sqrt", "MathSqrtActor"),
        ("tpl_math_absolute", "MathAbsoluteActor"),
        ("tpl_math_clamp", "MathClampActor"),
        ("tpl_math_min_max", "MathMinMaxActor"),
        ("tpl_math_round", "MathRoundActor"),
        ("tpl_math_random", "MathRandomActor"),
        ("tpl_math_average", "MathAverageActor"),
        ("tpl_math_sum", "MathSumActor"),
        ("tpl_math_statistics", "MathStatisticsActor"),
        ("tpl_math_expression", "MathExpressionActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }
    // Vector / Matrix / Quaternion
    for (id, name) in [
        ("tpl_vec3", "Vec3Actor"),
        ("tpl_vec3_add", "Vec3AddActor"),
        ("tpl_vec3_subtract", "Vec3SubtractActor"),
        ("tpl_vec3_scale", "Vec3ScaleActor"),
        ("tpl_vec3_dot", "Vec3DotActor"),
        ("tpl_vec3_cross", "Vec3CrossActor"),
        ("tpl_vec3_normalize", "Vec3NormalizeActor"),
        ("tpl_vec3_length", "Vec3LengthActor"),
        ("tpl_vec3_distance", "Vec3DistanceActor"),
        ("tpl_vec3_lerp", "Vec3LerpActor"),
        ("tpl_vec3_reflect", "Vec3ReflectActor"),
        ("tpl_mat4_identity", "Mat4IdentityActor"),
        ("tpl_mat4_multiply", "Mat4MultiplyActor"),
        ("tpl_mat4_transform", "Mat4TransformActor"),
        ("tpl_mat4_translate", "Mat4TranslateActor"),
        ("tpl_mat4_scale", "Mat4ScaleActor"),
        ("tpl_mat4_rotate_x", "Mat4RotateXActor"),
        ("tpl_mat4_rotate_y", "Mat4RotateYActor"),
        ("tpl_mat4_rotate_z", "Mat4RotateZActor"),
        ("tpl_mat4_look_at", "Mat4LookAtActor"),
        ("tpl_mat4_perspective", "Mat4PerspectiveActor"),
        ("tpl_quat_from_euler", "QuatFromEulerActor"),
        ("tpl_quat_multiply", "QuatMultiplyActor"),
        ("tpl_quat_slerp", "QuatSlerpActor"),
        ("tpl_quat_rotate_vec3", "QuatRotateVec3Actor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }

    // Procedural
    mapping.insert(
        "tpl_noise_generator".to_string(),
        "NoiseGeneratorActor".to_string(),
    );
    mapping.insert(
        "tpl_image_to_heightmap".to_string(),
        "ImageToHeightmapActor".to_string(),
    );
    mapping.insert(
        "tpl_heightmap_to_image".to_string(),
        "HeightmapToImageActor".to_string(),
    );
    mapping.insert(
        "tpl_heightmap_to_mesh".to_string(),
        "HeightmapToMeshActor".to_string(),
    );
    mapping.insert("tpl_voronoi".to_string(), "VoronoiActor".to_string());
    mapping.insert("tpl_lsystem".to_string(), "LSystemActor".to_string());
    mapping.insert(
        "tpl_particle_emitter".to_string(),
        "ParticleEmitterActor".to_string(),
    );
    mapping.insert(
        "tpl_triplanar_texture".to_string(),
        "TriplanarTextureActor".to_string(),
    );
    mapping.insert(
        "tpl_mesh_combine".to_string(),
        "MeshCombineActor".to_string(),
    );
    mapping.insert("tpl_tube_mesh".to_string(), "TubeMeshActor".to_string());
    mapping.insert(
        "tpl_vertex_color".to_string(),
        "VertexColorActor".to_string(),
    );
    mapping.insert("tpl_uv_texture".to_string(), "UVTextureActor".to_string());

    // Text / Utilities
    mapping.insert("tpl_json_parser".to_string(), "JsonParserActor".to_string());
    mapping.insert(
        "tpl_regex_matcher".to_string(),
        "RegexMatcherActor".to_string(),
    );
    mapping.insert("tpl_date_time".to_string(), "DateTimeActor".to_string());

    mapping.insert(
        "tpl_image_decode".to_string(),
        "ImageDecodeActor".to_string(),
    );
    mapping.insert(
        "tpl_image_encode".to_string(),
        "ImageEncodeActor".to_string(),
    );
    mapping.insert("tpl_file_load".to_string(), "FileLoadActor".to_string());
    mapping.insert("tpl_file_save".to_string(), "FileSaveActor".to_string());

    // SDF (always available)
    for (id, name) in [
        ("tpl_sdf_sphere", "SdfSphereActor"),
        ("tpl_sdf_box", "SdfBoxActor"),
        ("tpl_sdf_cylinder", "SdfCylinderActor"),
        ("tpl_sdf_torus", "SdfTorusActor"),
        ("tpl_sdf_capsule", "SdfCapsuleActor"),
        ("tpl_sdf_cone", "SdfConeActor"),
        ("tpl_sdf_plane", "SdfPlaneActor"),
        ("tpl_sdf_union", "SdfUnionActor"),
        ("tpl_sdf_intersection", "SdfIntersectionActor"),
        ("tpl_sdf_difference", "SdfDifferenceActor"),
        ("tpl_sdf_smooth_union", "SdfSmoothUnionActor"),
        ("tpl_sdf_smooth_intersection", "SdfSmoothIntersectionActor"),
        ("tpl_sdf_smooth_difference", "SdfSmoothDifferenceActor"),
        ("tpl_sdf_translate", "SdfTranslateActor"),
        ("tpl_sdf_rotate", "SdfRotateActor"),
        ("tpl_sdf_scale", "SdfScaleActor"),
        ("tpl_sdf_twist", "SdfTwistActor"),
        ("tpl_sdf_bend", "SdfBendActor"),
        ("tpl_sdf_round", "SdfRoundActor"),
        ("tpl_sdf_shell", "SdfShellActor"),
        ("tpl_sdf_mirror", "SdfMirrorActor"),
        ("tpl_sdf_repeat", "SdfRepeatActor"),
        ("tpl_sdf_displace", "SdfDisplaceActor"),
        ("tpl_sdf_material", "SdfMaterialActor"),
        ("tpl_sdf_path", "SdfPathActor"),
        ("tpl_sdf_scene", "SdfSceneActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }
    #[cfg(feature = "gpu")]
    {
        mapping.insert("tpl_sdf_render".to_string(), "SdfRenderActor".to_string());
        mapping.insert(
            "tpl_sdf_marching_cubes".to_string(),
            "SdfMarchingCubesActor".to_string(),
        );
        mapping.insert("tpl_mesh_to_sdf".to_string(), "MeshToSdfActor".to_string());
        mapping.insert(
            "tpl_scene_render".to_string(),
            "SceneRenderActor".to_string(),
        );
        mapping.insert(
            "tpl_gpu_2d_render".to_string(),
            "Gpu2DRenderActor".to_string(),
        );
    }
    mapping.insert("tpl_font_load".to_string(), "FontLoadActor".to_string());
    mapping.insert("tpl_glyph_atlas".to_string(), "GlyphAtlasActor".to_string());

    // Shader Graph
    for (id, name) in [
        ("tpl_shader_compiler", "ShaderCompilerActor"),
        ("tpl_shader_principled_bsdf", "ShaderPrincipledBsdfActor"),
        ("tpl_shader_material_output", "ShaderMaterialOutputActor"),
        ("tpl_shader_const_float", "ShaderConstFloatActor"),
        ("tpl_shader_const_color", "ShaderConstColorActor"),
        ("tpl_shader_texcoord", "ShaderTexCoordActor"),
        ("tpl_shader_position", "ShaderPositionInputActor"),
        ("tpl_shader_normal", "ShaderNormalInputActor"),
        ("tpl_shader_time", "ShaderTimeInputActor"),
        ("tpl_shader_vertex_color", "ShaderVertexColorActor"),
        ("tpl_shader_image_texture", "ShaderImageTextureActor"),
        ("tpl_shader_noise_texture", "ShaderNoiseTextureActor"),
        ("tpl_shader_checker_texture", "ShaderCheckerTextureActor"),
        ("tpl_shader_math", "ShaderMathActor"),
        ("tpl_shader_color_mix", "ShaderColorMixActor"),
        ("tpl_shader_color_ramp", "ShaderColorRampActor"),
        ("tpl_shader_fresnel", "ShaderFresnelActor"),
        ("tpl_shader_normal_map", "ShaderNormalMapActor"),
        ("tpl_shader_bump_map", "ShaderBumpMapActor"),
        ("tpl_shader_mapping", "ShaderMappingActor"),
        ("tpl_shader_separate_xyz", "ShaderSeparateXYZActor"),
        ("tpl_shader_combine_xyz", "ShaderCombineXYZActor"),
        ("tpl_shader_clamp", "ShaderClampActor"),
        ("tpl_shader_map_range", "ShaderMapRangeActor"),
        ("tpl_shader_voronoi_texture", "ShaderVoronoiTextureActor"),
        ("tpl_shader_gradient_texture", "ShaderGradientTextureActor"),
        ("tpl_shader_brick_texture", "ShaderBrickTextureActor"),
        ("tpl_shader_musgrave_texture", "ShaderMusgraveTextureActor"),
        ("tpl_shader_wave_texture", "ShaderWaveTextureActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }
    mapping.insert("tpl_obj_export".to_string(), "ObjExportActor".to_string());
    mapping.insert("tpl_stl_export".to_string(), "StlExportActor".to_string());
    mapping.insert("tpl_gltf_export".to_string(), "GltfExportActor".to_string());

    // Model/scene import
    for (id, name) in [
        ("tpl_stl_import", "StlImportActor"),
        ("tpl_obj_import", "ObjImportActor"),
        ("tpl_gltf_import", "GltfImportActor"),
        ("tpl_mesh_import", "MeshImportActor"),
        ("tpl_scene_import", "SceneImportActor"),
        ("tpl_fbx_import", "FbxImportActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }

    // Animation
    for (id, name) in [
        ("tpl_skeleton", "SkeletonActor"),
        ("tpl_animation_clip", "AnimationClipActor"),
        ("tpl_skin_bind", "SkinBindActor"),
        ("tpl_animation_sampler", "AnimationSamplerActor"),
        ("tpl_skinning", "SkinningActor"),
        ("tpl_animation_time", "AnimationTimeActor"),
        ("tpl_animation_mixer", "AnimationMixerActor"),
        ("tpl_keyframe", "KeyframeActor"),
        ("tpl_animation_timeline", "AnimationTimelineActor"),
        ("tpl_sprite_animation", "SpriteAnimationActor"),
        ("tpl_animation_blend_tree", "AnimationBlendTreeActor"),
        ("tpl_animation_fsm", "AnimationFsmActor"),
        ("tpl_ik_solver", "IKSolverActor"),
        ("tpl_root_motion", "RootMotionActor"),
        ("tpl_animation_layer", "AnimationLayerActor"),
        ("tpl_morph_target", "MorphTargetActor"),
        ("tpl_animation_event", "AnimationEventActor"),
        ("tpl_character_controller", "CharacterControllerActor"),
    ] {
        mapping.insert(id.to_string(), name.to_string());
    }

    // Video
    mapping.insert(
        "tpl_render_frame_collector".to_string(),
        "RenderFrameCollectorActor".to_string(),
    );
    #[cfg(feature = "video-encode")]
    mapping.insert(
        "tpl_video_encoder".to_string(),
        "VideoEncoderActor".to_string(),
    );

    // Audio DSP (continued)
    mapping.insert("tpl_equalizer".to_string(), "EqualizerActor".to_string());
    mapping.insert("tpl_limiter".to_string(), "LimiterActor".to_string());
    mapping.insert("tpl_dc_offset".to_string(), "DCOffsetActor".to_string());
    mapping.insert(
        "tpl_envelope_follower".to_string(),
        "EnvelopeFollowerActor".to_string(),
    );
    mapping.insert("tpl_crossover".to_string(), "CrossoverActor".to_string());
    mapping.insert("tpl_peak_detect".to_string(), "PeakDetectActor".to_string());
    mapping.insert("tpl_ifft".to_string(), "IFFTActor".to_string());
    mapping.insert("tpl_convolve".to_string(), "ConvolveActor".to_string());
    mapping.insert(
        "tpl_noise_reduction".to_string(),
        "NoiseReductionActor".to_string(),
    );
    mapping.insert("tpl_pitch_shift".to_string(), "PitchShiftActor".to_string());
    mapping.insert(
        "tpl_time_stretch".to_string(),
        "TimeStretchActor".to_string(),
    );
    mapping.insert("tpl_correlator".to_string(), "CorrelatorActor".to_string());

    // 2D Vector Graphics
    mapping.insert("tpl_shape_2d".to_string(), "Shape2DActor".to_string());
    mapping.insert(
        "tpl_vector_rasterize".to_string(),
        "VectorRasterizeActor".to_string(),
    );
    mapping.insert(
        "tpl_gaussian_blur".to_string(),
        "GaussianBlurActor".to_string(),
    );
    mapping.insert("tpl_blend_mode".to_string(), "BlendModeActor".to_string());
    mapping.insert("tpl_canvas_2d".to_string(), "Canvas2DActor".to_string());
    mapping.insert("tpl_background".to_string(), "BackgroundActor".to_string());

    mapping
}
