extern crate swc_ecma_parser;
use std::{
    collections::{BTreeMap, HashMap},
    env::args,
    io::{Read, stdin},
};

use anyhow::{Context, Result, anyhow};
use schemars::schema::{
    ArrayValidation, InstanceType, ObjectValidation, RootSchema, Schema, SchemaObject, SingleOrVec,
};
use swc_common::sync::Lrc;
use swc_common::{
    FileName, SourceMap,
    errors::{ColorConfig, Handler},
};
use swc_ecma_ast::{
    ImportDecl, TsArrayType, TsKeywordType, TsKeywordTypeKind, TsPropertySignature,
    TsType::{self, TsTypeLit},
    TsTypeAliasDecl, TsTypeAnn, TsTypeElement, TsTypeRef,
};
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};
use swc_ecma_visit::{Visit, VisitWith};

struct TypeVisitor {
    type_defs: HashMap<String, TsTypeAliasDecl>,
}

impl Visit for TypeVisitor {
    fn visit_ts_type_alias_decl(&mut self, node: &TsTypeAliasDecl) {
        self.type_defs.insert(node.id.sym.to_string(), node.clone());
        node.visit_children_with(self);
    }

    fn visit_import_decl(&mut self, _decl: &ImportDecl) {
        // for specifier in &decl.specifiers {
        //     if let swc_ecma_ast::ImportSpecifier::Named(named) = specifier {
        //         let Ident { sym, .. } = &named.local;
        //     }
        // }
        // decl.visit_children_with(self);
    }
}

fn main() -> Result<()> {
    let type_name = args().nth(1).context("TYPE_NAME expected")?;
    let input = read_stdin()?;
    let output = make_json_schema(&input, &type_name)?;
    println!("{}", output);
    Ok(())
}

fn read_stdin() -> Result<String> {
    let mut input = String::new();
    stdin().read_to_string(&mut input)?;
    Ok(input)
}

fn make_json_schema(input: &str, type_name: &str) -> Result<String> {
    let cm: Lrc<SourceMap> = Default::default();
    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));
    let source = cm.new_source_file(FileName::Custom("test.ts".into()).into(), input.into());
    let syntax = Syntax::Typescript(TsSyntax {
        tsx: true,
        decorators: true,
        dts: true,
        no_early_errors: true,
        disallow_ambiguous_jsx_like: true,
    });
    let lexer = Lexer::new(
        syntax,
        Default::default(),
        StringInput::from(source.as_ref()),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    for e in parser.take_errors() {
        e.into_diagnostic(&handler).emit();
    }
    let module = match parser.parse_module() {
        Ok(m) => m,
        Err(e) => return Err(anyhow!("Syntax error: {:?}", e.into_kind())),
    };

    let mut type_visitor = TypeVisitor {
        type_defs: HashMap::new(),
    };
    module.visit_with(&mut type_visitor);

    let schema = make_root_schema(type_name, &type_visitor.type_defs)?;
    let output = serde_json::to_string(&schema)?;

    Ok(output)
}

type Depenedencies = Vec<String>;

fn make_root_schema(
    type_name: &str,
    type_defs: &HashMap<String, TsTypeAliasDecl>,
) -> Result<RootSchema> {
    let mut definitions = BTreeMap::new();
    let mut dependencies: Depenedencies = vec![type_name.to_string()];
    while let Some(type_name) = dependencies.pop() {
        let (schema, deps) = make_schema(&type_name, type_defs)?;
        definitions.insert(type_name, schema);
        for dep in deps {
            if !dependencies.contains(&dep) {
                dependencies.push(dep)
            }
        }
    }
    Ok(RootSchema {
        meta_schema: Some("http://json-schema.org/draft-07/schema".into()),
        schema: SchemaObject {
            reference: Some(format!("#/definitions/{type_name}")),
            ..Default::default()
        },
        definitions,
    })
}

fn make_schema(
    type_name: &str,
    type_defs: &HashMap<String, TsTypeAliasDecl>,
) -> Result<(Schema, Depenedencies)> {
    {
        let instance_type = match type_name {
            "string" => Some(InstanceType::String),
            "boolean" => Some(InstanceType::Boolean),
            "number" => Some(InstanceType::Number),
            _ => None,
        };
        if instance_type.is_some() {
            return Ok((
                Schema::Object(SchemaObject {
                    instance_type: instance_type.map(|i| SingleOrVec::Single(Box::new(i))),
                    ..Default::default()
                }),
                vec![],
            ));
        }
    }
    let mut deps = vec![];
    let type_def = type_defs
        .get(type_name)
        .context(format!("no type def with name `{type_name}`"))?;
    let object = match type_def.type_ann.as_ref() {
        TsTypeLit(ts_type_lit) => {
            let mut object_val = ObjectValidation {
                ..Default::default()
            };
            for member in &ts_type_lit.members {
                match member {
                    TsTypeElement::TsPropertySignature(TsPropertySignature {
                        key,
                        type_ann,
                        ..
                    }) => {
                        let member_name = key.clone().ident().context("not ident")?.sym.to_string();
                        let ts_type_ann: &TsTypeAnn = type_ann.as_ref().unwrap();
                        let (member_object, mut member_deps) =
                            make_schema_from_ts_type(&ts_type_ann.type_ann)?;
                        deps.append(&mut member_deps);
                        object_val
                            .properties
                            .insert(member_name.clone(), member_object);
                    }
                    _ => todo!("{member:?}"),
                };
            }
            Schema::Object(SchemaObject {
                instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::Object))),
                object: Some(Box::new(object_val)),
                ..Default::default()
            })
        }
        t => todo!("{t:?}"),
    };
    Ok((object, deps))
}

fn make_schema_from_ts_type(ts_type: &TsType) -> Result<(Schema, Depenedencies)> {
    let mut deps = vec![];
    let schema = match ts_type {
        TsType::TsTypeRef(TsTypeRef { type_name, .. }) => {
            let type_name = type_name
                .clone()
                .ident()
                .context("not ident")?
                .sym
                .to_string();
            deps.push(type_name.clone());
            Schema::Object(SchemaObject {
                reference: Some(format!("#/definitions/{type_name}")),
                ..Default::default()
            })
        }
        TsType::TsKeywordType(TsKeywordType { kind, .. }) => {
            let type_name = match kind {
                TsKeywordTypeKind::TsNumberKeyword => "number",
                TsKeywordTypeKind::TsBooleanKeyword => "boolean",
                TsKeywordTypeKind::TsStringKeyword => "string",
                _ => todo!("{kind:?}"),
            };
            deps.push(type_name.into());
            Schema::Object(SchemaObject {
                reference: Some(format!("#/definitions/{type_name}")),
                ..Default::default()
            })
        }
        TsType::TsArrayType(TsArrayType { elem_type, .. }) => {
            let (array_schema, mut array_deps) = make_schema_from_ts_type(elem_type)?;
            deps.append(&mut array_deps);
            Schema::Object(SchemaObject {
                array: Some(Box::new(ArrayValidation {
                    items: Some(SingleOrVec::Single(Box::new(array_schema))),
                    ..Default::default()
                })),
                ..Default::default()
            })
        }
        ref t => todo!("{t:?}"),
    };
    Ok((schema, deps))
}

#[cfg(test)]
mod test {
    use crate::make_json_schema;
    use serde_json::{Value, json};

    fn assert_json_eq(a: &str, b: Value) {
        assert_eq!(serde_json::from_str::<Value>(a).unwrap(), b)
    }

    #[test]
    fn empty_object() {
        let schema = make_json_schema(
            "\
type A = {}",
            "A",
        )
        .unwrap();
        assert_json_eq(
            &schema,
            json!({
              "$schema": "http://json-schema.org/draft-07/schema",
              "$ref": "#/definitions/A",
              "definitions": {
                "A": {
                  "type": "object"
                }
              }
            }),
        );
    }

    #[test]
    fn object_with_primitives() {
        let schema = make_json_schema(
            "\
type A = {
    x: number;
    y: string;
    z: boolean
}",
            "A",
        )
        .unwrap();
        assert_json_eq(
            &schema,
            json!({
              "$schema": "http://json-schema.org/draft-07/schema",
              "$ref": "#/definitions/A",
              "definitions": {
                "A": {
                  "type": "object",
                  "properties": {
                    "x": {
                      "$ref": "#/definitions/number"
                    },
                    "y": {
                      "$ref": "#/definitions/string"
                    },
                    "z": {
                      "$ref": "#/definitions/boolean"
                    }
                  }
                },
                "boolean": {
                  "type": "boolean"
                },
                "number": {
                  "type": "number"
                },
                "string": {
                  "type": "string"
                }
              }
            }),
        );
    }

    #[test]
    fn nested_object() {
        let schema = make_json_schema(
            "\
type A = {
    b: B
}
type B = {}",
            "A",
        )
        .unwrap();
        assert_json_eq(
            &schema,
            json!({
              "$schema": "http://json-schema.org/draft-07/schema",
              "$ref": "#/definitions/A",
              "definitions": {
                "A": {
                  "type": "object",
                  "properties": {
                      "b": {
                          "$ref": "#/definitions/B"
                      }
                  }
                },
                "B": {
                  "type": "object"
                }
              }
            }),
        );
    }
}
