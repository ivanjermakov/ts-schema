extern crate swc_ecma_parser;
use std::{
    collections::HashMap,
    env::args,
    io::{Read, stdin},
};

use anyhow::{Context, Result, anyhow};
use swc_common::sync::Lrc;
use swc_common::{
    FileName, SourceMap,
    errors::{ColorConfig, Handler},
};
use swc_ecma_ast::{Ident, ImportDecl, TsTypeAliasDecl};
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
        //         eprintln!("{}: {:#?}", sym, decl);
        //     }
        // }
        // decl.visit_children_with(self);
    }
}

fn main() -> Result<()> {
    let cm: Lrc<SourceMap> = Default::default();
    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));

    let type_name = args().nth(1).context("TYPE_NAME expected")?;

    let input = read_stdin()?;

    let source = cm.new_source_file(FileName::Custom("test.ts".into()).into(), input);
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

    // eprintln!("{:#?}", module);

    let mut type_visitor = TypeVisitor {
        type_defs: HashMap::new(),
    };
    module.visit_with(&mut type_visitor);
    eprintln!("{:?}", type_visitor.type_defs.keys().collect::<Vec<_>>());

    Ok(())
}

fn read_stdin() -> Result<String> {
    let mut input = String::new();
    stdin().read_to_string(&mut input)?;
    Ok(input)
}
