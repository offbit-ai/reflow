use crate::PythonRuntime;
use anyhow::Result;
use core::time;
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_script_execution() -> Result<()> {
        let mut runtime = PythonRuntime::new(false, true);
        runtime.init(uuid::Uuid::new_v4(), None, None).await?;

        let result = runtime
            .execute(
                uuid::Uuid::new_v4(),
                r#"print('Hello, World!')
__return_value = 2 + 2"#,
                HashMap::new(),
                None,
                None,
            )
            .await?;

        
        assert_eq!(result.result, Some(json!(4)));
        assert!(result.success);
        Ok(())
    }

    #[tokio::test]
    async fn test_package_installation() -> Result<()> {
        let mut runtime = PythonRuntime::new(false, true);
        runtime
            .init(uuid::Uuid::new_v4(), Some(vec!["requests".to_string()]), None)
            .await?;

        let result = runtime
            .execute(
                uuid::Uuid::new_v4(),
                "import requests\n__return_value=requests.__version__",
                HashMap::new(),
                None,
                None,
            )
            .await?;

        assert!(result.success);
        assert!(result.packages_installed.contains(&"requests".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn test_input_parameters() -> Result<()> {
        let mut runtime = PythonRuntime::new(false, true);
        runtime.init(uuid::Uuid::new_v4(), None, None).await?;

        let mut inputs = HashMap::new();
        inputs.insert("x".to_string(), json!(5));
        inputs.insert("y".to_string(), json!(3));

        let result = runtime
            .execute(uuid::Uuid::new_v4(), r#"
inputs = Context.get_inputs()
x = inputs.get("x").data
y = inputs.get("y").data
__return_value=x + y           
            "#, inputs, None, None)
            .await?;

        assert_eq!(result.result, Some(json!(8)));
        assert!(result.success);
        Ok(())
    }

    #[tokio::test]
    async fn test_execution_timeout() {
        let mut runtime = PythonRuntime::new(false, true);
        runtime.init(uuid::Uuid::new_v4(), None, None).await.unwrap();

        let result = runtime
            .execute(
                uuid::Uuid::new_v4(),
                "import time\ntime.sleep(2)\n__return_value=42",
                HashMap::new(),
                Some(time::Duration::from_secs(1)),
                None,
            )
            .await;

        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_syntax_error()-> Result<()> {
        let mut runtime = PythonRuntime::new(false, true);
        runtime.init(uuid::Uuid::new_v4(), None, None).await.unwrap();

        let result = runtime
            .execute(
                uuid::Uuid::new_v4(),
                "if True print('invalid syntax')",
                HashMap::new(),
                None,
                None,
            )
            .await?;

        assert!(result.stderr.contains("invalid syntax"));
        Ok(())
    }

    #[tokio::test]
    async fn test_stdout_capture() -> Result<()> {
        let mut runtime = PythonRuntime::new(false, true);
        runtime.init(uuid::Uuid::new_v4(), None, None).await?;

        let result = runtime
            .execute(
                uuid::Uuid::new_v4(),
                "print('captured output')",
                HashMap::new(),
                None,
                None,
            )
            .await?;
        
        assert!(result.stdout.trim().contains("captured output"));
        assert!(result.success);
        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_executions() -> Result<()> {
        let mut runtime = PythonRuntime::new(false, true);
        let session_id = uuid::Uuid::new_v4();
        runtime.init(session_id, None, None).await?;

        let result1 = runtime
            .execute(session_id, "x = 42", HashMap::new(), None, None)
            .await?;

        let result2 = runtime
            .execute(session_id, "x + 8", HashMap::new(), None, None)
            .await?;

        assert!(result1.success);
        assert!(result2.success);
        assert_eq!(result2.result, Some(json!(50)));
        Ok(())
    }

    #[tokio::test]
    async fn test_message_passing() -> Result<()> {
        let mut runtime = PythonRuntime::new(false, true);
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
}
