use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

use coasonix_runtime_core::{
    kernel::{AuditEvent, RuntimeConfig, RuntimeKernel, SchemaValidationRequest},
    policy::{CommandInvocation, PermissionLevel, ResourceSet, RuntimeOperationRequest},
};
use serde::Deserialize;
use serde_json::{Value, json};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut worker = Worker::default();

    worker.serve(stdin.lock(), stdout.lock());
}

#[derive(Default)]
struct Worker {
    kernel: Option<RuntimeKernel>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct InitializeParams {
    repo_root: PathBuf,
    schema_path: PathBuf,
    reasonix_executable: String,
}

#[derive(Debug, Deserialize)]
struct ValidateSchemaParams {
    task_id: String,
    request_id: Option<String>,
    expected_schema: String,
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct EvaluateOperationParams {
    task_id: String,
    request_id: Option<String>,
    operation: String,
    permission_level: String,
    resources: WorkerResources,
}

#[derive(Debug, Deserialize)]
struct WorkerResources {
    #[serde(default)]
    read_paths: Vec<String>,
    #[serde(default)]
    write_paths: Vec<String>,
    #[serde(default)]
    network: bool,
    command: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct WriteAuditParams {
    task_id: String,
    event_type: String,
    summary: String,
    payload_json: String,
}

#[derive(Debug, Clone)]
struct JsonRpcError {
    code: i64,
    message: &'static str,
}

impl Worker {
    fn serve(&mut self, input: impl BufRead, mut output: impl Write) {
        for line in input.lines() {
            let response = match line {
                Ok(line) => self.handle_line(&line),
                Err(_) => Some(error_response(None, runtime_internal_error())),
            };

            if let Some(response) = response {
                let _ = writeln!(output, "{response}");
                let _ = output.flush();
                if response
                    .get("result")
                    .and_then(|result| result.get("shutdown"))
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    break;
                }
            }
        }
    }

    fn handle_line(&mut self, line: &str) -> Option<Value> {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return Some(error_response(None, parse_error())),
        };

        let request: JsonRpcRequest = match serde_json::from_value(value) {
            Ok(request) => request,
            Err(_) => return Some(error_response(None, invalid_request())),
        };

        let Some(id) = request.id else {
            return Some(error_response(None, invalid_request()));
        };

        if request.jsonrpc != "2.0" {
            return Some(error_response(Some(id), invalid_request()));
        }

        let result = self.dispatch(&request.method, request.params, &id);
        Some(match result {
            Ok(result) => success_response(id, result),
            Err(error) => error_response(Some(id), error),
        })
    }

    fn dispatch(&mut self, method: &str, params: Value, id: &Value) -> Result<Value, JsonRpcError> {
        match method {
            "runtime.initialize" => self.initialize(params),
            "runtime.validate_schema" => self.validate_schema(params, id),
            "runtime.evaluate_operation" => self.evaluate_operation(params, id),
            "runtime.write_audit" => self.write_audit(params),
            "runtime.shutdown" => Ok(json!({ "shutdown": true })),
            _ => Err(method_not_found()),
        }
    }

    fn initialize(&mut self, params: Value) -> Result<Value, JsonRpcError> {
        let params: InitializeParams =
            serde_json::from_value(params).map_err(|_| invalid_params())?;
        let kernel = RuntimeKernel::initialize(RuntimeConfig {
            repo_root: params.repo_root,
            schema_path: params.schema_path,
            reasonix_executable: params.reasonix_executable,
        })
        .map_err(|_| runtime_unavailable())?;
        self.kernel = Some(kernel);
        Ok(json!({ "initialized": true }))
    }

    fn validate_schema(&self, params: Value, id: &Value) -> Result<Value, JsonRpcError> {
        let params: ValidateSchemaParams =
            serde_json::from_value(params).map_err(|_| invalid_params())?;
        let request_id = params.request_id.or_else(|| request_id_from_jsonrpc_id(id));
        let kernel = self.kernel.as_ref().ok_or_else(runtime_unavailable)?;
        let result = kernel.validate_schema(SchemaValidationRequest {
            task_id: params.task_id.clone(),
            request_id: request_id.clone(),
            expected_schema: params.expected_schema,
            payload: params.payload,
        });
        Ok(result.to_payload(&params.task_id, request_id.as_deref()))
    }

    fn evaluate_operation(&mut self, params: Value, id: &Value) -> Result<Value, JsonRpcError> {
        let params: EvaluateOperationParams =
            serde_json::from_value(params).map_err(|_| invalid_params())?;
        let request_id = params.request_id.or_else(|| request_id_from_jsonrpc_id(id));
        let permission_level = permission_level_from_str(&params.permission_level)?;
        let kernel = self.kernel.as_mut().ok_or_else(runtime_unavailable)?;
        let decision = kernel.evaluate_operation(RuntimeOperationRequest {
            task_id: params.task_id,
            request_id,
            operation: params.operation,
            permission_level,
            resources: ResourceSet {
                read_paths: params.resources.read_paths,
                write_paths: params.resources.write_paths,
                network: params.resources.network,
                command: params.resources.command.map(CommandInvocation::Argv),
            },
        });
        Ok(decision.to_payload())
    }

    fn write_audit(&mut self, params: Value) -> Result<Value, JsonRpcError> {
        let params: WriteAuditParams =
            serde_json::from_value(params).map_err(|_| invalid_params())?;
        let kernel = self.kernel.as_mut().ok_or_else(runtime_unavailable)?;
        let record = kernel
            .write_audit(AuditEvent {
                task_id: params.task_id,
                event_type: params.event_type,
                summary: params.summary,
                payload_json: params.payload_json,
            })
            .map_err(|_| runtime_unavailable())?;
        Ok(json!({
            "id": record.id,
            "task_sequence": record.task_sequence
        }))
    }
}

fn success_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn error_response(id: Option<Value>, error: JsonRpcError) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": error.code,
            "message": error.message
        }
    })
}

fn request_id_from_jsonrpc_id(id: &Value) -> Option<String> {
    id.as_str()
        .filter(|value| value.starts_with("REQ-"))
        .map(ToString::to_string)
}

fn permission_level_from_str(value: &str) -> Result<PermissionLevel, JsonRpcError> {
    match value {
        "L0_READONLY" => Ok(PermissionLevel::L0Readonly),
        "L1_DIFF_REVIEW" => Ok(PermissionLevel::L1DiffReview),
        "L2_PATCH_ONLY" => Ok(PermissionLevel::L2PatchOnly),
        "L3_ISOLATED_WORKTREE" => Ok(PermissionLevel::L3IsolatedWorktree),
        _ => Err(invalid_params()),
    }
}

fn parse_error() -> JsonRpcError {
    JsonRpcError {
        code: -32700,
        message: "Parse error",
    }
}

fn invalid_request() -> JsonRpcError {
    JsonRpcError {
        code: -32600,
        message: "Invalid Request",
    }
}

fn method_not_found() -> JsonRpcError {
    JsonRpcError {
        code: -32601,
        message: "Method not found",
    }
}

fn invalid_params() -> JsonRpcError {
    JsonRpcError {
        code: -32602,
        message: "Invalid params",
    }
}

fn runtime_unavailable() -> JsonRpcError {
    JsonRpcError {
        code: -32008,
        message: "runtime_unavailable",
    }
}

fn runtime_internal_error() -> JsonRpcError {
    JsonRpcError {
        code: -32010,
        message: "runtime_internal_error",
    }
}
