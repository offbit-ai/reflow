//! End-to-End Integration Tests
//!
//! Tests the complete flow: Zeal node metadata → Actor execution

#[cfg(test)]
mod tests {
    use crate::integration::HttpRequestActor;
    use crate::{get_actor_for_template, registry, Actor};
    use reflow_actor::{ActorConfig, ActorContext, MemoryState};
    use reflow_graph::types::GraphNode;
    use serde_json::json;
    use std::collections::HashMap;

    fn create_http_node() -> GraphNode {
        GraphNode {
            id: "http_node".to_string(),
            component: "tpl_http_request".to_string(),
            metadata: Some({
                let mut metadata = HashMap::new();
                metadata.insert(
                    "propertyValues".to_string(),
                    json!({
                        "url": "https://jsonplaceholder.typicode.com/posts/1",
                        "method": "GET",
                        "timeout": 30000,
                    }),
                );
                metadata
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_zeal_node_structure() {
        let http_node = create_http_node();

        assert_eq!(http_node.id, "http_node");
        assert_eq!(http_node.component, "tpl_http_request");

        let metadata = http_node.metadata.as_ref().unwrap();
        let property_values = metadata.get("propertyValues").unwrap().as_object().unwrap();
        assert_eq!(
            property_values.get("url").unwrap().as_str().unwrap(),
            "https://jsonplaceholder.typicode.com/posts/1"
        );
        assert_eq!(
            property_values.get("method").unwrap().as_str().unwrap(),
            "GET"
        );
        assert_eq!(
            property_values.get("timeout").unwrap().as_u64().unwrap(),
            30000
        );
    }

    #[tokio::test]
    async fn test_registry_actor_mapping() {
        let http_actor = get_actor_for_template("tpl_http_request");
        assert!(
            http_actor.is_some(),
            "Should find actor for tpl_http_request"
        );

        let if_actor = get_actor_for_template("tpl_if_branch");
        assert!(if_actor.is_some(), "Should find actor for tpl_if_branch");

        let switch_actor = get_actor_for_template("tpl_switch");
        assert!(switch_actor.is_some(), "Should find actor for tpl_switch");

        let loop_actor = get_actor_for_template("tpl_loop");
        assert!(loop_actor.is_some(), "Should find actor for tpl_loop");

        let transform_actor = get_actor_for_template("tpl_data_transformer");
        assert!(
            transform_actor.is_some(),
            "Should find actor for tpl_data_transformer"
        );

        let rules_actor = get_actor_for_template("tpl_rules_engine");
        assert!(
            rules_actor.is_some(),
            "Should find actor for tpl_rules_engine"
        );

        // Script templates should NOT be in the native registry
        let js_actor = get_actor_for_template("tpl_javascript_script");
        assert!(
            js_actor.is_none(),
            "Script actors should be handled by dynASB, not native registry"
        );

        let unknown_actor = get_actor_for_template("tpl_unknown");
        assert!(
            unknown_actor.is_none(),
            "Should not find actor for unknown template"
        );
    }

    #[tokio::test]
    #[ignore] // requires network access
    async fn test_http_node_to_actor_execution() {
        let node = create_http_node();

        let actor = get_actor_for_template(&node.component);
        assert!(actor.is_some(), "Should find HTTP actor");

        let actor = actor.unwrap();
        let behavior = actor.get_behavior();

        let config = ActorConfig {
            node: node.clone(),
            resolved_env: HashMap::new(),
            config: node.metadata.clone().unwrap_or_default(),
            namespace: None,
            inport_connection_counts: HashMap::new(),
        };

        let payload = HashMap::new();
        let outports = flume::unbounded();
        let state = std::sync::Arc::new(parking_lot::Mutex::new(MemoryState::default()));
        let load = std::sync::Arc::new(reflow_actor::ActorLoad::new(0));

        let context = ActorContext::new(payload, outports, state, config, load);

        let result = behavior(context).await;
        assert!(result.is_ok(), "HTTP actor execution should succeed");

        let output = result.unwrap();
        assert!(
            output.contains_key("response_out") || output.contains_key("error_out"),
            "Should have response_out or error_out"
        );
    }

    #[tokio::test]
    async fn test_template_mapping_completeness() {
        let expected_templates = vec![
            "tpl_http_request",
            "tpl_if_branch",
            "tpl_switch",
            "tpl_loop",
            "tpl_data_transformer",
            "tpl_data_operations",
            "tpl_rules_engine",
            "tpl_image_input",
            "tpl_audio_input",
            "tpl_video_input",
        ];

        for template in &expected_templates {
            let actor = get_actor_for_template(template);
            assert!(
                actor.is_some(),
                "Missing actor implementation for template: {}",
                template
            );
        }

        for template in &expected_templates {
            assert!(
                mapping.contains_key(*template),
                "{} template should be mapped",
                template
            );
        }
    }

    #[tokio::test]
    #[cfg(feature = "ml")]
    async fn test_ml_template_mapping() {
        let expected_templates = vec![
            "tpl_cv_image_to_tensor",
            "tpl_cv_resize_letterbox",
            "tpl_cv_normalize_tensor",
            "tpl_cv_tensor_crop_roi",
            "tpl_cv_detection_to_roi",
            "tpl_cv_temporal_smoother",
            "tpl_ml_load_model",
            "tpl_ml_run_inference",
            "tpl_ml_decode_detections",
            "tpl_ml_decode_landmarks",
            "tpl_ml_packet_probe",
        ];

        let mapping = registry::get_template_mapping();
        for template in &expected_templates {
            assert!(
                get_actor_for_template(template).is_some(),
                "Missing ML actor implementation for template: {}",
                template
            );
            assert!(
                mapping.contains_key(*template),
                "{} ML template should be mapped",
                template
            );
        }
    }

    #[tokio::test]
    #[ignore] // requires network access
    async fn test_actor_metadata_processing() {
        let node = create_http_node();

        let http_actor = HttpRequestActor::new();
        let behavior = http_actor.get_behavior();

        let config = ActorConfig {
            node: node.clone(),
            resolved_env: HashMap::new(),
            config: node.metadata.clone().unwrap_or_default(),
            namespace: None,
            inport_connection_counts: HashMap::new(),
        };

        let payload = HashMap::new();
        let outports = flume::unbounded();
        let state = std::sync::Arc::new(parking_lot::Mutex::new(MemoryState::default()));
        let load = std::sync::Arc::new(reflow_actor::ActorLoad::new(0));

        let context = ActorContext::new(payload, outports, state, config, load);

        let result = behavior(context).await;
        assert!(result.is_ok(), "Actor should process metadata correctly");

        let output = result.unwrap();
        assert!(
            output.contains_key("response_out") || output.contains_key("error_out"),
            "Should generate correct output ports"
        );
    }
}
