use crate::PythonRuntime;
use anyhow::Result;
use flume;
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
mod remote_tests {
    use std::env;

    use super::*;

    #[tokio::test]
    async fn test_remote_initialization() -> Result<()> {
        let mut runtime = PythonRuntime::new(true, true);
        runtime.init(uuid::Uuid::new_v4(), None, None).await?;
        assert!(runtime.remote_client.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn test_remote_script_execution() -> Result<()> {
        let mut runtime = PythonRuntime::new(true, true);
        let session_id = uuid::Uuid::new_v4();
        runtime.init(session_id, None, None).await?;

        let result = runtime
            .execute(session_id, "2 ** 3", HashMap::new(), None, None)
            .await?;

        assert_eq!(result.result, Some(json!(8)));
        assert!(result.success);
        Ok(())
    }

    #[tokio::test]
    async fn test_remote_package_installation() -> Result<()> {
        let mut runtime = PythonRuntime::new(true, true);
        let session_id = uuid::Uuid::new_v4();
        runtime
            .init(session_id, Some(vec!["numpy".to_string()]), None)
            .await?;

        let result = runtime
            .execute(
                session_id,
                "import numpy as np\n__return_value=np.array([1, 2, 3]).sum()",
                HashMap::new(),
                None,
                None,
            )
            .await?;

        assert_eq!(result.result, Some(json!(6)));
        assert!(result.packages_installed.contains(&"numpy".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn test_remote_message_passing() -> Result<()> {
        let mut runtime = PythonRuntime::new(true, true);
        let session_id = uuid::Uuid::new_v4();
        runtime.init(session_id, None, None).await?;

        let (tx, rx) = flume::unbounded();

        let result = runtime
            .execute(
                session_id,
                r#"
import json
for i in range(3):
    Context.send_output({"progress": i})
__return_value = 42"#,
                HashMap::new(),
                None,
                Some(tx),
            )
            .await?;

        let mut messages = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        assert!(!messages.is_empty());
        assert_eq!(result.result, Some(json!(42)));
        Ok(())
    }

    #[tokio::test]
    async fn test_remote_error_handling() -> Result<()> {
        let mut runtime = PythonRuntime::new(true, true);
        let session_id = uuid::Uuid::new_v4();
        runtime.init(session_id, None, None).await.unwrap();

        let result = runtime
            .execute(
                session_id,
                "raise ValueError('test error')",
                HashMap::new(),
                None,
                None,
            )
            .await?;

        assert!(result.stderr.contains("ValueError"));
        Ok(())
    }

    #[tokio::test]
    async fn test_remote_complex_data() -> Result<()> {
        let mut runtime = PythonRuntime::new(true, true);
        let session_id = uuid::Uuid::new_v4();
        // unsafe {std::env::set_var("RUST_LOG", "debug,tokio_tungstenite=debug,tungstenite=debug")};
        // env_logger::init();
        runtime.init(session_id, None, None).await?;

        let mut inputs = HashMap::new();
        inputs.insert(
            "data".to_string(),
            json!({
                "numbers": [1, 2, 3],
                "text": "test",
                "nested": {"value": true}
            }),
        );

        let (tx, rx) = flume::unbounded();

        let result = runtime
            .execute(
                session_id,
                r#"
inputs = Context.get_inputs()
data = inputs.get("data").data
data['numbers'].append(4)
data['text'] = data['text'].upper()
__return_value=data
                "#,
                inputs,
                
                None,
                Some(tx),
            )
            .await?;

        let binding = result.result.unwrap();
        let result_obj = binding.as_object().unwrap();
        assert_eq!(result_obj["text"], "TEST");
        assert_eq!(result_obj["numbers"], json!([1, 2, 3, 4]));

        // while let Ok(_) = rx.recv_async().await {
            
        // }

        Ok(())
    }
}
