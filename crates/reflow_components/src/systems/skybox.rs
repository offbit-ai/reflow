//! Skybox system — reads :skybox components, outputs sky rendering data.
//!
//! ## Component schema: `entity:skybox`
//!
//! ### Cubemap skybox
//! ```json
//! {
//!   "mode": "cubemap",
//!   "faces": {
//!     "px": "sky_right:texture",
//!     "nx": "sky_left:texture",
//!     "py": "sky_top:texture",
//!     "ny": "sky_bottom:texture",
//!     "pz": "sky_front:texture",
//!     "nz": "sky_back:texture"
//!   },
//!   "rotation": 0.0,
//!   "exposure": 1.0
//! }
//! ```
//!
//! ### Procedural sky (Preetham/Hosek-Wilkie model)
//! ```json
//! {
//!   "mode": "procedural",
//!   "sunDirection": [0.3, 0.8, 0.5],
//!   "turbidity": 2.0,
//!   "rayleigh": 1.0,
//!   "mieCoefficient": 0.005,
//!   "mieDirectionalG": 0.8,
//!   "exposure": 1.0
//! }
//! ```
//!
//! ### Gradient sky
//! ```json
//! {
//!   "mode": "gradient",
//!   "topColor": [0.1, 0.15, 0.4],
//!   "horizonColor": [0.5, 0.6, 0.8],
//!   "bottomColor": [0.2, 0.2, 0.2],
//!   "exponent": 2.0,
//!   "offset": 0.0
//! }
//! ```
//!
//! ### HDRI environment map
//! ```json
//! {
//!   "mode": "hdri",
//!   "texture": "studio_env:texture",
//!   "rotation": 0.0,
//!   "exposure": 1.5,
//!   "blur": 0.0
//! }
//! ```
//!
//! ## Output
//!
//! Writes `:skybox_data` with render-ready parameters. The render
//! system uses this to draw the sky behind all geometry.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    SceneSkyboxSystemActor,
    inports::<10>(tick, entity_id),
    outports::<1>(skybox_data, metadata),
    state(MemoryState)
)]
pub async fn scene_skybox_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let db = get_or_create_db(db_path)?;

    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let skybox_entities = if selected.is_empty() {
        db.entities_with(&["skybox"])?
    } else {
        selected
            .into_iter()
            .filter(|e| db.has_component(e, "skybox"))
            .collect()
    };

    // Use the first active skybox
    let mut sky_output = json!(null);

    for entity in &skybox_entities {
        let sky_asset = match db.get_component(entity, "skybox") {
            Ok(a) => a,
            Err(_) => continue,
        };
        let sky: Value = sky_asset
            .entry
            .inline_data
            .unwrap_or_else(|| serde_json::from_slice(&sky_asset.data).unwrap_or(json!({})));

        let mode = sky
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("gradient");
        let exposure = sky.get("exposure").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let rotation = sky.get("rotation").and_then(|v| v.as_f64()).unwrap_or(0.0);

        let data = match mode {
            "cubemap" => {
                json!({
                    "mode": "cubemap",
                    "faces": sky.get("faces"),
                    "rotation": rotation,
                    "exposure": exposure,
                })
            }
            "hdri" => {
                json!({
                    "mode": "hdri",
                    "texture": sky.get("texture"),
                    "rotation": rotation,
                    "exposure": exposure,
                    "blur": sky.get("blur").and_then(|v| v.as_f64()).unwrap_or(0.0),
                })
            }
            "procedural" => {
                let sun_dir = sky
                    .get("sunDirection")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        [
                            a.first().and_then(|v| v.as_f64()).unwrap_or(0.3),
                            a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.8),
                            a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.5),
                        ]
                    })
                    .unwrap_or([0.3, 0.8, 0.5]);
                let turbidity = sky.get("turbidity").and_then(|v| v.as_f64()).unwrap_or(2.0);
                let rayleigh = sky.get("rayleigh").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let mie_coeff = sky
                    .get("mieCoefficient")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.005);
                let mie_g = sky
                    .get("mieDirectionalG")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.8);

                // Compute sky colors from sun position (simplified Preetham)
                let sun_y = sun_dir[1].max(0.0).min(1.0);
                let day_factor = sun_y;
                let sunset_factor = (1.0 - (sun_y - 0.1).abs() * 5.0).max(0.0);

                let zenith = [
                    0.1 + 0.15 * day_factor + 0.4 * sunset_factor,
                    0.15 + 0.35 * day_factor + 0.1 * sunset_factor,
                    0.3 + 0.4 * day_factor - 0.1 * sunset_factor,
                ];
                let horizon = [
                    0.5 + 0.3 * sunset_factor,
                    0.55 + 0.15 * day_factor + 0.1 * sunset_factor,
                    0.6 + 0.2 * day_factor - 0.15 * sunset_factor,
                ];

                json!({
                    "mode": "procedural",
                    "sunDirection": sun_dir,
                    "turbidity": turbidity,
                    "rayleigh": rayleigh,
                    "mieCoefficient": mie_coeff,
                    "mieDirectionalG": mie_g,
                    "exposure": exposure,
                    "zenithColor": zenith,
                    "horizonColor": horizon,
                    "dayFactor": day_factor,
                    "sunsetFactor": sunset_factor,
                })
            }
            _ => {
                // Gradient (default)
                let top = sky
                    .get("topColor")
                    .and_then(|v| v.as_array())
                    .map(|a| [fv(a, 0, 0.1), fv(a, 1, 0.15), fv(a, 2, 0.4)])
                    .unwrap_or([0.1, 0.15, 0.4]);
                let horizon = sky
                    .get("horizonColor")
                    .and_then(|v| v.as_array())
                    .map(|a| [fv(a, 0, 0.5), fv(a, 1, 0.6), fv(a, 2, 0.8)])
                    .unwrap_or([0.5, 0.6, 0.8]);
                let bottom = sky
                    .get("bottomColor")
                    .and_then(|v| v.as_array())
                    .map(|a| [fv(a, 0, 0.2), fv(a, 1, 0.2), fv(a, 2, 0.2)])
                    .unwrap_or([0.2, 0.2, 0.2]);
                let exponent = sky.get("exponent").and_then(|v| v.as_f64()).unwrap_or(2.0);
                let offset = sky.get("offset").and_then(|v| v.as_f64()).unwrap_or(0.0);

                json!({
                    "mode": "gradient",
                    "topColor": top,
                    "horizonColor": horizon,
                    "bottomColor": bottom,
                    "exponent": exponent,
                    "offset": offset,
                    "exposure": exposure,
                })
            }
        };

        let _ = db.set_component_json(entity, "skybox_data", data.clone(), json!({}));
        sky_output = data;
        break; // Use first skybox
    }

    let mut out = HashMap::new();
    out.insert(
        "skybox_data".to_string(),
        Message::object(EncodableValue::from(sky_output)),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "skyboxCount": skybox_entities.len(),
        }))),
    );
    Ok(out)
}

fn fv(a: &[Value], idx: usize, default: f64) -> f64 {
    a.get(idx).and_then(|v| v.as_f64()).unwrap_or(default)
}
