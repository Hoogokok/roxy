/// JSON 설정 스키마 정의
/// 
/// 이 모듈은 JSON 설정 파일의 검증에 사용되는 JSON 스키마를 정의합니다.
/// 스키마는 JSON Schema Draft 7을 따릅니다.

/// JSON 설정 스키마 상수
pub const CONFIG_SCHEMA: &str = r#"{
    "$schema": "http://json-schema.org/draft-07/schema#",
    "type": "object",
    "required": ["version"],
    "properties": {
        "version": {
            "type": "string",
            "enum": ["1.0"]
        },
        "id": {
            "type": "string"
        },
        "server": {
            "type": "object",
            "properties": {
                "http_port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "https_port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "https_enabled": {"type": "boolean"},
                "retry_count": {"type": "integer", "minimum": 0},
                "retry_interval": {"type": "integer", "minimum": 0}
            }
        },
        "middlewares": {
            "type": "object",
            "additionalProperties": {
                "type": "object",
                "required": ["type"],
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["basic-auth", "cors", "ratelimit", "headers", "compress"]
                    },
                    "users": {
                        "type": "array",
                        "items": {"type": "string"}
                    },
                    "allow_origins": {
                        "type": "array",
                        "items": {"type": "string"}
                    },
                    "allow_methods": {
                        "type": "array",
                        "items": {"type": "string"}
                    },
                    "average": {"type": "integer", "minimum": 0},
                    "burst": {"type": "integer", "minimum": 0},
                    "headers": {
                        "type": "object",
                        "additionalProperties": {"type": "string"}
                    }
                }
            }
        },
        "routers": {
            "type": "object",
            "additionalProperties": {
                "type": "object",
                "required": ["rule", "service"],
                "properties": {
                    "rule": {"type": "string"},
                    "service": {"type": "string"},
                    "middlewares": {
                        "type": "array",
                        "items": {"type": "string"}
                    },
                    "priority": {"type": "integer"}
                }
            }
        },
        "services": {
            "type": "object",
            "additionalProperties": {
                "type": "object",
                "required": ["loadbalancer"],
                "properties": {
                    "loadbalancer": {
                        "type": "object",
                        "required": ["servers"],
                        "properties": {
                            "servers": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "required": ["url"],
                                    "properties": {
                                        "url": {"type": "string", "format": "uri"},
                                        "weight": {"type": "integer", "minimum": 1}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        "health": {
            "type": "object",
            "properties": {
                "enabled": {"type": "boolean"},
                "interval": {"type": "integer", "minimum": 1},
                "timeout": {"type": "integer", "minimum": 1},
                "max_failures": {"type": "integer", "minimum": 0},
                "http": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    }
                }
            }
        },
        "router_middlewares": {
            "type": "object",
            "additionalProperties": {
                "type": "array",
                "items": {"type": "string"}
            }
        }
    }
}"#; 