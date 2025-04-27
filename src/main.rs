extern crate swc_ecma_parser;
use swc_common::sync::Lrc;
use swc_common::{
    FileName, SourceMap,
    errors::{ColorConfig, Handler},
};
use swc_ecma_parser::{Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

fn main() {
    let cm: Lrc<SourceMap> = Default::default();
    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));

    // Real usage
    // let fm = cm
    //     .load_file(Path::new("test.js"))
    //     .expect("failed to load test.js");
    let fm = cm.new_source_file(
        FileName::Custom("test.ts".into()).into(),
        "type Foo = {}".into(),
    );
    let lexer = Lexer::new(
        Syntax::Typescript(TsSyntax {
            tsx: true,
            decorators: true,
            dts: true,
            no_early_errors: true,
            disallow_ambiguous_jsx_like: true,
        }),
        Default::default(),
        StringInput::from(fm.as_ref()),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    for e in parser.take_errors() {
        e.into_diagnostic(&handler).emit();
    }

    let module = parser
        .parse_module()
        .map_err(|e| e.into_diagnostic(&handler).emit())
        .expect("failed to parser module");

    println!("{:?}", module);
}
