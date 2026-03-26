use serde_json::{json, Value};

/// MCP tool definition.
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Return all MCP tool definitions. ALL ARE READ-ONLY.
pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "slg_why".to_string(),
            description: "Search git history semantically. Find why decisions were made."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Semantic search query (max 500 chars)",
                        "maxLength": 500
                    },
                    "limit": {
                        "type": "number",
                        "description": "Number of results (default 3, max 10)",
                        "default": 3,
                        "maximum": 10
                    },
                    "since": {
                        "type": "string",
                        "description": "Filter commits after this ISO date"
                    },
                    "author": {
                        "type": "string",
                        "description": "Filter by author name"
                    },
                    "format": {
                        "type": "string",
                        "enum": ["xml", "json"],
                        "default": "xml"
                    },
                    "max_tokens": {
                        "type": "number",
                        "description": "Maximum response tokens (default 4096)",
                        "default": 4096
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "slg_blame".to_string(),
            description: "Find semantic ownership of a file or function.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "File path to analyze"
                    },
                    "fn": {
                        "type": "string",
                        "description": "Function name to focus on"
                    },
                    "risk": {
                        "type": "boolean",
                        "description": "Include risk score",
                        "default": false
                    }
                },
                "required": ["file"]
            }),
        },
        ToolDefinition {
            name: "slg_log".to_string(),
            description: "Search git history grouped by intent.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "since": {
                        "type": "string",
                        "description": "Filter commits after this ISO date"
                    },
                    "by_intent": {
                        "type": "boolean",
                        "description": "Group results by intent",
                        "default": false
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "slg_bisect".to_string(),
            description: "Find which commit likely introduced a bug.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "bug_description": {
                        "type": "string",
                        "description": "Description of the bug"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Max candidates to return",
                        "default": 5
                    }
                },
                "required": ["bug_description"]
            }),
        },
        ToolDefinition {
            name: "slg_status".to_string(),
            description: "Get current slg index status.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

/// Convert tool definitions to JSON for MCP tools/list response.
pub fn tools_list_response() -> Value {
    let tools: Vec<Value> = get_tool_definitions()
        .into_iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema
            })
        })
        .collect();

    json!({ "tools": tools })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions() {
        let tools = get_tool_definitions();
        assert_eq!(tools.len(), 5);
        assert_eq!(tools[0].name, "slg_why");
        assert_eq!(tools[4].name, "slg_status");
    }

    #[test]
    fn test_tools_list_response() {
        let resp = tools_list_response();
        let tools = resp["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 5);
    }
}
