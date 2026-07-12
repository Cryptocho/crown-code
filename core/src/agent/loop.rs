use std::io::{self, Write};

use crate::agent::prompt::build_system_prompt;
use crate::agent::tools::{execute_tool, get_tool_definitions};
use crate::api::openai::{create_message_stream, new_client};
use crate::api::types::*;

pub async fn run_agent_loop(config: ApiClientConfig) {
    let mut client = new_client(config);
    let tools = get_tool_definitions();
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let system_prompt = build_system_prompt(&cwd);
    let mut history: Vec<Message> = Vec::new();

    history.push(Message {
        role: MessageRole::System,
        content: system_prompt.clone(),
        tool_calls: Vec::new(),
        tool_call_id: String::new(),
        name: String::new(),
    });
    eprintln!(
        "[PROMPT] System prompt ({} chars):\n{}\n---",
        system_prompt.len(),
        system_prompt
    );

    println!("crown-code — A vibe coding tool");
    println!("Type your task. /quit to exit.");

    loop {
        print!("\nYou: ");
        io::stdout().flush().unwrap();

        let mut user_input = String::new();
        match io::stdin().read_line(&mut user_input) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let user_input = user_input.trim().to_string();
        if user_input.is_empty() {
            continue;
        }
        if user_input == "/quit" || user_input == "/exit" {
            break;
        }

        history.push(Message {
            role: MessageRole::User,
            content: user_input,
            tool_calls: Vec::new(),
            tool_call_id: String::new(),
            name: String::new(),
        });

        loop {
            let mut assistant_text = String::new();

            print!("\nAssistant: ");
            io::stdout().flush().unwrap();

            let resp = create_message_stream(&mut client, &history, &tools, |chunk| {
                match chunk {
                    ApiStreamChunk::Text(t) => {
                        print!("{}", t);
                        io::stdout().flush().unwrap();
                        assistant_text.push_str(&t);
                    }
                    ApiStreamChunk::Reasoning(_) | ApiStreamChunk::Usage { .. } | ApiStreamChunk::Done => {}
                    ApiStreamChunk::ToolCall(_) => {}
                }
                true
            }).await;

            if !resp.error.message.is_empty() {
                println!("\n[API Error] {}", resp.error.message);
                break;
            }

            let tool_calls = resp.tool_calls;

            history.push(Message {
                role: MessageRole::Assistant,
                content: assistant_text,
                tool_calls: tool_calls.clone(),
                tool_call_id: String::new(),
                name: String::new(),
            });

            if tool_calls.is_empty() {
                println!();
                break;
            }

            eprintln!("[TOOL_CALL] Model requested {} tool call(s):", tool_calls.len());
            for tc in &tool_calls {
                eprintln!("  - {}({})", tc.function_name, tc.arguments);
            }
            eprintln!("---");

            let mut has_completion = false;
            for tc in &tool_calls {
                let args: serde_json::Value = match serde_json::from_str(&tc.arguments) {
                    Ok(v) => v,
                    Err(_) => {
                        let error_msg = format!(
                            "Error: tool call arguments are invalid JSON (truncated response?): {}",
                            tc.arguments
                        );
                        eprintln!("[TOOL_RESULT] {}\n---", error_msg);
                        history.push(Message {
                            role: MessageRole::Tool,
                            content: error_msg,
                            tool_calls: Vec::new(),
                            tool_call_id: tc.id.clone(),
                            name: tc.function_name.clone(),
                        });
                        continue;
                    }
                };

                let result = execute_tool(&tc.function_name, &args).await;

                eprintln!("[TOOL_RESULT] {}:\n{}\n---", tc.function_name, result);

                print!("\n  [{}]\n", tc.function_name);
                io::stdout().flush().unwrap();

                if tc.function_name == "attempt_completion" {
                    has_completion = true;
                }

                history.push(Message {
                    role: MessageRole::Tool,
                    content: result,
                    tool_calls: Vec::new(),
                    tool_call_id: tc.id.clone(),
                    name: tc.function_name.clone(),
                });
            }

            if has_completion {
                println!("\n--- Task finished. Enter new task or /quit ---");
                break;
            }
        }
    }
}