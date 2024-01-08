use std::collections::HashMap;
use std::fmt::Error;

use async_trait::async_trait;
use cainome::parser::tokens::{Composite, Token};

use crate::error::BindgenResult;
use crate::plugins::BuiltinPlugin;
use crate::{DojoMetadata, DojoModel};

#[derive(Debug)]
pub enum UnityPluginError {
    InvalidType(String),
}

impl std::fmt::Display for UnityPluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            UnityPluginError::InvalidType(type_path) => write!(f, "Invalid type: {}", type_path),
        }
    }
}

impl std::error::Error for UnityPluginError {}

pub struct UnityPlugin {
}

impl UnityPlugin {
    pub fn new() -> Self {
        Self {
        }
    }

    // Maps cairo types to C#/Unity SDK defined types
    fn map_type(type_name: &str) -> Result<String, UnityPluginError> {
        match type_name {
            "u8" => Ok("byte".to_string()),
            "u16" => Ok("ushort".to_string()),
            "u32" => Ok("uint".to_string()),
            "u64" => Ok("ulong".to_string()),
            "u128" => Ok("Span<byte>".to_string()),
            "u256" => Ok("Span<ulong>".to_string()),
            "usize" => Ok("uint".to_string()),
            "felt252" => Ok("FieldElement".to_string()),
            "ClassHash" => Ok("FieldElement".to_string()),
            "ContractAddress" => Ok("FieldElement".to_string()),

            _ => Ok(type_name.to_string()),
        }
    }

    // Token should be a struct
    // This will be formatted into a C# struct
    // using C# and unity SDK types
    fn format_struct(token: &Composite) -> Result<String, UnityPluginError> {
        let fields = token.inners.iter().map(|field| {
            format!(
                "public {} {};",
                UnityPlugin::map_type(field.token.clone().type_name().as_str()).unwrap(),
                field.name
            )
        }).collect::<Vec<String>>().join("\n    ");

        return Ok(format!(
            "
[Serializable]
public struct {} {{
    {}
}}
",
            token.type_name(), fields
        ));
    }

    fn format_model(token: &Composite) -> Result<String, UnityPluginError> {
        let fields = token.inners.iter().map(|field| {
            format!(
                "[ModelField(\"{}\")]\n    public {} {};",
                field.name,
                UnityPlugin::map_type(field.token.clone().type_name().as_str()).unwrap(),
                field.name
            )
        }).collect::<Vec<String>>().join("\n\n    ");

        return Ok(format!(
            "
public class {} : ModelInstance {{
    {}

    // Start is called before the first frame update
    void Start() {{
    }}

    // Update is called once per frame
    void Update() {{
    }}
}}
        ",
            token.type_name(), fields
        ));
    }

    fn handle_model(&self, token: Composite, tokens: &HashMap<String, Vec<Token>>) -> Result<String, UnityPluginError> {
        let mut out = String::new();
        out += "using System;\n";
        out += "using Dojo;\n";
        out += "using Dojo.Starknet;\n";
        
        let structs = tokens.get("structs").unwrap();
        for field in &token.inners {
            if let Token::Composite(c) = &field.token {
                for struct_token in structs {
                    if struct_token.type_name() == c.type_name() {
                        out += UnityPlugin::format_struct(struct_token.to_composite().unwrap())?.as_str();
                    }
                }
            }
        }

        out += "\n\n";

        out += UnityPlugin::format_model(&token)?.as_str();

        Ok(out)
    }
    
    fn handle_enum(&self, token: Token) {}

    fn handle_function(&self, token: Token) {}
}

#[async_trait]
impl BuiltinPlugin for UnityPlugin {
    async fn generate_systems_bindings(
        &self,
        contract_name: &str,
        tokens_map: HashMap<String, Vec<Token>>,
        metadata: &DojoMetadata,
    ) -> BindgenResult<()> {
        // we have 3 token types
        // funcitons, enums and structs
        for (token_type, tokens) in &tokens_map {
            match token_type.as_str() {
                "structs" => {
                    for token in tokens {
                        if let Some(model) = metadata.models.get(token.type_name().as_str()) {
                            let model = self.handle_model(token.to_composite().unwrap().clone(), &tokens_map).unwrap();
                            println!("{}", model);
                        }
                    }
                }
                "enums" => {
                    for token in tokens {
                        // self.handle_enum(token);
                    }
                }
                "functions" => {
                    for token in tokens {
                        // self.handle_function(token);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}
