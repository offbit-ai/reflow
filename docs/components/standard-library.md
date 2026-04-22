# Standard Component Library

Reflow's standard library provides native actor implementations exposed through `reflow_components`. These are the templates discoverable via `get_actor_for_template(template_id)` and `get_template_mapping()`.

> **If you are building an application, depend on [`reflow_rt`](https://crates.io/crates/reflow_rt)** — it re-exports the catalog as `reflow_rt::components` and owns the feature gates (`gpu`, `av-core`, `ml`, `camera-native`, `video-encode`, `window-events`, `browser-events`, `api_services`, …).

Script execution (JavaScript, Python, SQL, etc.) is handled by dynASB via `ComponentSpec::Script` — this crate only contains native actors.

## Registry usage

```rust
use reflow_rt::prelude::*;

let actor = get_actor_for_template("tpl_http_request")
    .expect("template registered");

net.register_actor_arc("tpl_http_request", actor)?;
net.add_node("call", "tpl_http_request", Some(/* config */))?;
```

`get_template_mapping()` returns `HashMap<String, String>` of template ID → actor struct name for tools and editors.

## Feature gates

| Feature | What it enables |
|---------|-----------------|
| `av-core` | Audio / DSP actors (biquad, compressor, FFT, gain, spectrum, etc.) |
| `gpu` | wgpu-backed rendering: scene render, SDF ray march, shader graph, post-processing |
| `window-events` | `tpl_*_input` and `tpl_window_event` |
| `browser-events` / `browser` | Browser automation actors |
| `camera-native` | Native camera capture (`tpl_camera_capture`) |
| `video-encode` | Native H.264 video encoding (`tpl_video_encoder`) |
| `ml` | CV preprocess, inference boundary, decode actors, taskpacks |
| `api_services` | ~6,700 generated API actors across ~90 third-party services |

## Complete template catalog

The tables below are organized by the sections in `registry.rs`. Feature-gated actors are noted in their section heading — they are only resolvable when the matching feature is enabled.

The API-services catalog (`api_*` templates) is not listed here because of its size; see **[api-actors.md](./api-actors.md)** for the full list.

For the media / ML pipeline stack see **[ml-stack.md](./ml-stack.md)** and **[media-actors.md](./media-actors.md)**.

---

### Asset DB

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_asset_store` | AssetStoreActor | asset store |
| `tpl_asset_load` | AssetLoadActor | asset load |
| `tpl_asset_query` | AssetQueryActor | asset query |

### Scene Systems (ECS — read/write AssetDB components)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_scene_physics` | ScenePhysicsSystemActor | scene physics |
| `tpl_scene_camera` | SceneCameraSystemActor | scene camera |
| `tpl_scene_light_collector` | SceneLightCollectorActor | scene light collector |
| `tpl_scene_material` | SceneMaterialSystemActor | scene material |
| `tpl_scene_billboard` | SceneBillboardSystemActor | scene billboard |
| `tpl_scene_skybox` | SceneSkyboxSystemActor | scene skybox |
| `tpl_scene_weather` | SceneWeatherSystemActor | scene weather |

### Universal Systems (motion design, interactive animation, design engineering)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_tween_system` | TweenSystemActor | tween system |
| `tpl_timeline_system` | TimelineSystemActor | timeline system |
| `tpl_state_machine_system` | StateMachineSystemActor | state machine system |
| `tpl_behavior_system` | BehaviorSystemActor | behavior system |
| `tpl_layout_sync` | LayoutSyncSystemActor | layout sync |
| `tpl_text_render` | TextRenderSystemActor | text render |
| `tpl_text_sdf` | TextSdfSystemActor | text sdf |

### Integration

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_http_request` | HttpRequestActor | http request |
| `tpl_browser_screencast` | BrowserScreencastActor | browser screencast |

### Flow Control

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_fsm` | FsmActor | fsm |
| `tpl_hit_test` | HitTestActor | hit test |
| `tpl_signal` | SignalActor | signal |
| `tpl_subscriber` | SubscriberActor | subscriber |
| `tpl_if_branch` | ConditionalBranchActor | if branch |
| `tpl_switch` | SwitchCaseActor | switch |
| `tpl_loop` | LoopActor | loop |

### Scene

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_component` | ComponentNodeActor | component |
| `tpl_prefab` | PrefabActor | prefab |
| `tpl_instance` | InstanceActor | instance |
| `tpl_scene_graph` | SceneGraphActor | scene graph |
| `tpl_terrain` | TerrainActor | terrain |

### Input Events (feature-gated)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_keyboard_input` | KeyboardInputActor | keyboard input |
| `tpl_mouse_input` | MouseInputActor | mouse input |
| `tpl_gamepad_input` | GamepadInputActor | gamepad input |
| `tpl_touch_input` | TouchInputActor | touch input |
| `tpl_window_event` | WindowEventActor | window event |

### Triggers

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_interval_trigger` | IntervalTriggerActor | interval trigger |
| `tpl_cron_trigger` | CronTriggerActor | cron trigger |

### Server

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_server_request` | ServerRequestActor | server request |
| `tpl_server_response` | ServerResponseActor | server response |

### Flow Utilities

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_map` | MapActor | map |
| `tpl_filter` | FilterActor | filter |
| `tpl_reduce` | ReduceActor | reduce |
| `tpl_merge` | MergeActor | merge |
| `tpl_split` | SplitActor | split |
| `tpl_delay` | DelayActor | delay |
| `tpl_gate` | GateActor | gate |
| `tpl_collect` | CollectActor | collect |
| `tpl_passthrough` | PassthroughActor | passthrough |

### Data Processing

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_data_emit` | DataEmitActor | data emit |
| `tpl_data_transformer` | DataTransformActor | data transformer |
| `tpl_data_operations` | DataOperationsActor | data operations |
| `tpl_generator` | GeneratorActor | generator |

### Logic

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_rules_engine` | RulesEngineActor | rules engine |

### Media

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_image_input` | ImageInputActor | image input |
| `tpl_audio_input` | AudioInputActor | audio input |
| `tpl_video_input` | VideoInputActor | video input |
| `tpl_camera_capture` | CameraCaptureActor | camera capture |

### Math

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_math_add` | MathAddActor | math add |
| `tpl_math_subtract` | MathSubtractActor | math subtract |
| `tpl_math_multiply` | MathMultiplyActor | math multiply |
| `tpl_math_divide` | MathDivideActor | math divide |
| `tpl_math_modulo` | MathModuloActor | math modulo |
| `tpl_math_power` | MathPowerActor | math power |
| `tpl_math_sqrt` | MathSqrtActor | math sqrt |
| `tpl_math_absolute` | MathAbsoluteActor | math absolute |
| `tpl_math_clamp` | MathClampActor | math clamp |
| `tpl_math_min_max` | MathMinMaxActor | math min max |
| `tpl_math_round` | MathRoundActor | math round |
| `tpl_math_random` | MathRandomActor | math random |
| `tpl_math_average` | MathAverageActor | math average |
| `tpl_math_sum` | MathSumActor | math sum |
| `tpl_math_statistics` | MathStatisticsActor | math statistics |
| `tpl_math_expression` | MathExpressionActor | math expression |

### Vector3

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_vec3` | Vec3Actor | vec3 |
| `tpl_vec3_add` | Vec3AddActor | vec3 add |
| `tpl_vec3_subtract` | Vec3SubtractActor | vec3 subtract |
| `tpl_vec3_scale` | Vec3ScaleActor | vec3 scale |
| `tpl_vec3_dot` | Vec3DotActor | vec3 dot |
| `tpl_vec3_cross` | Vec3CrossActor | vec3 cross |
| `tpl_vec3_normalize` | Vec3NormalizeActor | vec3 normalize |
| `tpl_vec3_length` | Vec3LengthActor | vec3 length |
| `tpl_vec3_distance` | Vec3DistanceActor | vec3 distance |
| `tpl_vec3_lerp` | Vec3LerpActor | vec3 lerp |
| `tpl_vec3_reflect` | Vec3ReflectActor | vec3 reflect |

### Matrix4

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_mat4_identity` | Mat4IdentityActor | mat4 identity |
| `tpl_mat4_multiply` | Mat4MultiplyActor | mat4 multiply |
| `tpl_mat4_transform` | Mat4TransformActor | mat4 transform |
| `tpl_mat4_translate` | Mat4TranslateActor | mat4 translate |
| `tpl_mat4_scale` | Mat4ScaleActor | mat4 scale |
| `tpl_mat4_rotate_x` | Mat4RotateXActor | mat4 rotate x |
| `tpl_mat4_rotate_y` | Mat4RotateYActor | mat4 rotate y |
| `tpl_mat4_rotate_z` | Mat4RotateZActor | mat4 rotate z |
| `tpl_mat4_look_at` | Mat4LookAtActor | mat4 look at |
| `tpl_mat4_perspective` | Mat4PerspectiveActor | mat4 perspective |

### Quaternion

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_quat_from_euler` | QuatFromEulerActor | quat from euler |
| `tpl_quat_multiply` | QuatMultiplyActor | quat multiply |
| `tpl_quat_slerp` | QuatSlerpActor | quat slerp |
| `tpl_quat_rotate_vec3` | QuatRotateVec3Actor | quat rotate vec3 |

### Procedural

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_noise_generator` | NoiseGeneratorActor | noise generator |

### Procedural / Heightmap

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_image_to_heightmap` | ImageToHeightmapActor | image to heightmap |
| `tpl_heightmap_to_image` | HeightmapToImageActor | heightmap to image |
| `tpl_heightmap_to_mesh` | HeightmapToMeshActor | heightmap to mesh |
| `tpl_voronoi` | VoronoiActor | voronoi |
| `tpl_lsystem` | LSystemActor | lsystem |
| `tpl_particle_emitter` | ParticleEmitterActor | particle emitter |
| `tpl_triplanar_texture` | TriplanarTextureActor | triplanar texture |
| `tpl_mesh_combine` | MeshCombineActor | mesh combine |
| `tpl_tube_mesh` | TubeMeshActor | tube mesh |
| `tpl_vertex_color` | VertexColorActor | vertex color |
| `tpl_uv_texture` | UVTextureActor | uv texture |

### Text / Utilities

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_json_parser` | JsonParserActor | json parser |
| `tpl_regex_matcher` | RegexMatcherActor | regex matcher |
| `tpl_date_time` | DateTimeActor | date time |

### Image Codecs

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_image_decode` | ImageDecodeActor | image decode |
| `tpl_image_encode` | ImageEncodeActor | image encode |

### File I/O

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_file_load` | FileLoadActor | file load |
| `tpl_file_save` | FileSaveActor | file save |

### Stream Display

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_image_stream_display` | ImageStreamDisplayActor | image stream display |
| `tpl_audio_stream_display` | AudioStreamDisplayActor | audio stream display |

### Stream Operations

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_bytes_to_stream` | BytesToStreamActor | bytes to stream |
| `tpl_stream_to_bytes` | StreamToBytesActor | stream to bytes |
| `tpl_stream_tee` | StreamTeeActor | stream tee |
| `tpl_stream_buffer` | StreamBufferActor | stream buffer |
| `tpl_stream_throttle` | StreamThrottleActor | stream throttle |
| `tpl_stream_stats` | StreamStatsActor | stream stats |

### Image DSP

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_grayscale_filter` | GrayscaleFilterActor | grayscale filter |
| `tpl_brightness_contrast` | BrightnessContrastActor | brightness contrast |
| `tpl_chroma_key` | ChromaKeyActor | chroma key |

### Audio DSP

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_audio_gain` | AudioGainActor | audio gain |
| `tpl_biquad_filter` | BiquadFilterActor | biquad filter |
| `tpl_compressor` | CompressorActor | compressor |
| `tpl_audio_normalize` | AudioNormalizeActor | audio normalize |
| `tpl_noise_gate` | NoiseGateActor | noise gate |
| `tpl_de_esser` | DeEsserActor | de esser |
| `tpl_audio_spectrum` | AudioSpectrumActor | audio spectrum |
| `tpl_silence_detect` | SilenceDetectActor | silence detect |

### Audio DSP (continued)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_equalizer` | EqualizerActor | equalizer |
| `tpl_limiter` | LimiterActor | limiter |
| `tpl_dc_offset` | DCOffsetActor | dc offset |
| `tpl_envelope_follower` | EnvelopeFollowerActor | envelope follower |
| `tpl_crossover` | CrossoverActor | crossover |
| `tpl_peak_detect` | PeakDetectActor | peak detect |
| `tpl_ifft` | IFFTActor | ifft |
| `tpl_convolve` | ConvolveActor | convolve |
| `tpl_noise_reduction` | NoiseReductionActor | noise reduction |
| `tpl_pitch_shift` | PitchShiftActor | pitch shift |
| `tpl_time_stretch` | TimeStretchActor | time stretch |
| `tpl_correlator` | CorrelatorActor | correlator |

### Image DSP (continued)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_image_resize` | ImageResizeActor | image resize |

### SDF (always available — pure IR composition)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_sdf_sphere` | SdfSphereActor | sdf sphere |
| `tpl_sdf_box` | SdfBoxActor | sdf box |
| `tpl_sdf_round_box` | SdfRoundBoxActor | sdf round box |
| `tpl_sdf_ellipsoid` | SdfEllipsoidActor | sdf ellipsoid |
| `tpl_sdf_round_box_shell` | SdfRoundBoxShellActor | sdf round box shell |
| `tpl_sdf_cylinder` | SdfCylinderActor | sdf cylinder |
| `tpl_sdf_torus` | SdfTorusActor | sdf torus |
| `tpl_sdf_capsule` | SdfCapsuleActor | sdf capsule |
| `tpl_sdf_cone` | SdfConeActor | sdf cone |
| `tpl_sdf_tapered_capsule` | SdfTaperedCapsuleActor | sdf tapered capsule |
| `tpl_sdf_tube_path` | SdfTubePathActor | sdf tube path |
| `tpl_sdf_plane` | SdfPlaneActor | sdf plane |
| `tpl_sdf_inf_repeat` | SdfInfRepeatActor | sdf inf repeat |
| `tpl_sdf_puddle` | SdfPuddleActor | sdf puddle |
| `tpl_sdf_union` | SdfUnionActor | sdf union |
| `tpl_sdf_intersection` | SdfIntersectionActor | sdf intersection |
| `tpl_sdf_difference` | SdfDifferenceActor | sdf difference |
| `tpl_sdf_smooth_union` | SdfSmoothUnionActor | sdf smooth union |
| `tpl_sdf_smooth_intersection` | SdfSmoothIntersectionActor | sdf smooth intersection |
| `tpl_sdf_smooth_difference` | SdfSmoothDifferenceActor | sdf smooth difference |
| `tpl_sdf_stamp_compose` | SdfStampComposeActor | sdf stamp compose |
| `tpl_sdf_translate` | SdfTranslateActor | sdf translate |
| `tpl_sdf_rotate` | SdfRotateActor | sdf rotate |
| `tpl_sdf_scale` | SdfScaleActor | sdf scale |
| `tpl_sdf_twist` | SdfTwistActor | sdf twist |
| `tpl_sdf_bend` | SdfBendActor | sdf bend |
| `tpl_sdf_round` | SdfRoundActor | sdf round |
| `tpl_sdf_shell` | SdfShellActor | sdf shell |
| `tpl_sdf_mirror` | SdfMirrorActor | sdf mirror |
| `tpl_sdf_repeat` | SdfRepeatActor | sdf repeat |
| `tpl_sdf_displace` | SdfDisplaceActor | sdf displace |
| `tpl_sdf_material` | SdfMaterialActor | sdf material |
| `tpl_sdf_shade_slot` | SdfShadeSlotActor | sdf shade slot |
| `tpl_sdf_scene` | SdfSceneActor | sdf scene |

### SDF path (always available — pure IR composition)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_sdf_path` | SdfPathActor | sdf path |

### GPU compute (requires wgpu)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_sdf_live_render` | SdfLiveRenderActor | sdf live render |
| `tpl_sdf_render` | SdfRenderActor | sdf render |
| `tpl_sdf_marching_cubes` | SdfMarchingCubesActor | sdf marching cubes |
| `tpl_mesh_to_sdf` | MeshToSdfActor | mesh to sdf |
| `tpl_scene_render` | SceneRenderActor | scene render |
| `tpl_gpu_2d_render` | Gpu2DRenderActor | gpu 2d render |
| `tpl_font_load` | FontLoadActor | font load |
| `tpl_glyph_atlas` | GlyphAtlasActor | glyph atlas |

### Post-processing

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_tone_map` | ToneMapActor | tone map |
| `tpl_bloom` | BloomPostProcessActor | bloom |
| `tpl_ssao` | SSAOActor | ssao |
| `tpl_shadow_map` | ShadowMapActor | shadow map |

### Shader Graph (node-based materials)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_shader_compiler` | ShaderCompilerActor | shader compiler |
| `tpl_shader_principled_bsdf` | ShaderPrincipledBsdfActor | shader principled bsdf |
| `tpl_shader_material_output` | ShaderMaterialOutputActor | shader material output |
| `tpl_shader_const_float` | ShaderConstFloatActor | shader const float |
| `tpl_shader_const_color` | ShaderConstColorActor | shader const color |
| `tpl_shader_texcoord` | ShaderTexCoordActor | shader texcoord |
| `tpl_shader_position` | ShaderPositionInputActor | shader position |
| `tpl_shader_normal` | ShaderNormalInputActor | shader normal |
| `tpl_shader_time` | ShaderTimeInputActor | shader time |
| `tpl_shader_vertex_color` | ShaderVertexColorActor | shader vertex color |
| `tpl_shader_image_texture` | ShaderImageTextureActor | shader image texture |
| `tpl_shader_noise_texture` | ShaderNoiseTextureActor | shader noise texture |
| `tpl_shader_checker_texture` | ShaderCheckerTextureActor | shader checker texture |
| `tpl_shader_math` | ShaderMathActor | shader math |
| `tpl_shader_color_mix` | ShaderColorMixActor | shader color mix |
| `tpl_shader_color_ramp` | ShaderColorRampActor | shader color ramp |
| `tpl_shader_fresnel` | ShaderFresnelActor | shader fresnel |
| `tpl_shader_normal_map` | ShaderNormalMapActor | shader normal map |
| `tpl_shader_bump_map` | ShaderBumpMapActor | shader bump map |
| `tpl_shader_mapping` | ShaderMappingActor | shader mapping |
| `tpl_shader_separate_xyz` | ShaderSeparateXYZActor | shader separate xyz |
| `tpl_shader_combine_xyz` | ShaderCombineXYZActor | shader combine xyz |
| `tpl_shader_clamp` | ShaderClampActor | shader clamp |
| `tpl_shader_map_range` | ShaderMapRangeActor | shader map range |
| `tpl_shader_voronoi_texture` | ShaderVoronoiTextureActor | shader voronoi texture |
| `tpl_shader_gradient_texture` | ShaderGradientTextureActor | shader gradient texture |
| `tpl_shader_brick_texture` | ShaderBrickTextureActor | shader brick texture |
| `tpl_shader_musgrave_texture` | ShaderMusgraveTextureActor | shader musgrave texture |
| `tpl_shader_wave_texture` | ShaderWaveTextureActor | shader wave texture |

### Animation

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_skeleton` | SkeletonActor | skeleton |
| `tpl_animation_clip` | AnimationClipActor | animation clip |
| `tpl_skin_bind` | SkinBindActor | skin bind |
| `tpl_animation_sampler` | AnimationSamplerActor | animation sampler |
| `tpl_skinning` | SkinningActor | skinning |
| `tpl_animation_time` | AnimationTimeActor | animation time |
| `tpl_animation_mixer` | AnimationMixerActor | animation mixer |
| `tpl_keyframe` | KeyframeActor | keyframe |
| `tpl_animation_timeline` | AnimationTimelineActor | animation timeline |
| `tpl_sprite_animation` | SpriteAnimationActor | sprite animation |
| `tpl_animation_blend_tree` | AnimationBlendTreeActor | animation blend tree |
| `tpl_animation_fsm` | AnimationFsmActor | animation fsm |
| `tpl_ik_solver` | IKSolverActor | ik solver |
| `tpl_root_motion` | RootMotionActor | root motion |
| `tpl_animation_layer` | AnimationLayerActor | animation layer |
| `tpl_morph_target` | MorphTargetActor | morph target |
| `tpl_animation_event` | AnimationEventActor | animation event |
| `tpl_character_controller` | CharacterControllerActor | character controller |

### Video

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_frame_buffer` | ? | frame buffer |
| `tpl_render_frame_collector` | RenderFrameCollectorActor | render frame collector |
| `tpl_video_encoder` | VideoEncoderActor | video encoder |

### Mesh export

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_obj_export` | ObjExportActor | obj export |
| `tpl_stl_export` | StlExportActor | stl export |
| `tpl_gltf_export` | GltfExportActor | gltf export |

### Model/scene import

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_stl_import` | StlImportActor | stl import |
| `tpl_obj_import` | ObjImportActor | obj import |
| `tpl_gltf_import` | GltfImportActor | gltf import |
| `tpl_mesh_import` | MeshImportActor | mesh import |
| `tpl_scene_import` | SceneImportActor | scene import |
| `tpl_fbx_import` | FbxImportActor | fbx import |

### 2D Vector Graphics

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_shape_2d` | Shape2DActor | shape 2d |
| `tpl_vector_rasterize` | VectorRasterizeActor | vector rasterize |
| `tpl_gaussian_blur` | GaussianBlurActor | gaussian blur |
| `tpl_blend_mode` | BlendModeActor | blend mode |
| `tpl_canvas_2d` | Canvas2DActor | canvas 2d |
| `tpl_background` | BackgroundActor | background |

### Media / ML stack (feature-gated; mock-first inference boundary)

| Template ID | Actor | Purpose |
|-------------|-------|---------|
| `tpl_cv_image_to_tensor` | ImageToTensorActor | cv image to tensor |
| `tpl_cv_resize_letterbox` | ResizeLetterboxActor | cv resize letterbox |
| `tpl_cv_video_stream_to_frames` | VideoStreamToFramesActor | cv video stream to frames |
| `tpl_cv_normalize_tensor` | NormalizeTensorActor | cv normalize tensor |
| `tpl_cv_tensor_crop_roi` | TensorCropRoiActor | cv tensor crop roi |
| `tpl_cv_detection_to_roi` | DetectionToRoiActor | cv detection to roi |
| `tpl_cv_temporal_smoother` | TemporalSmootherActor | cv temporal smoother |
| `tpl_ml_load_model` | LoadModelActor | ml load model |
| `tpl_ml_run_inference` | RunInferenceActor | ml run inference |
| `tpl_ml_decode_detections` | DecodeDetectionsActor | ml decode detections |
| `tpl_ml_decode_landmarks` | DecodeLandmarksActor | ml decode landmarks |
| `tpl_ml_packet_probe` | PacketProbeActor | ml packet probe |
