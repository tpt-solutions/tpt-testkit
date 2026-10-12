//! Procedural macros for tpt-testkit (suite test generation).
//!
//! Use the re-exported macro, `tpt_testkit::suite::tpt_suite_tests!`. It is not meant to be
//! depended on directly.

use std::collections::BTreeSet;
use std::path::PathBuf;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use serde::Deserialize;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Path, Token, parse_macro_input};

/// Only the part of the suite manifest the macro needs. Full validation happens at test runtime
/// in `tpt_testkit::suite::Manifest::from_json`.
#[derive(Deserialize)]
struct ManifestFile {
    files: Vec<ManifestEntry>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    path: String,
}

struct SuiteTests {
    manifest: LitStr,
    check: Path,
}

impl Parse for SuiteTests {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let manifest: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let key: Ident = input.parse()?;
        if key != "check" {
            return Err(syn::Error::new(
                key.span(),
                "expected `check = <function path>`",
            ));
        }
        input.parse::<Token![=]>()?;
        let check: Path = input.parse()?;
        let _trailing: Option<Token![,]> = input.parse()?;
        Ok(Self { manifest, check })
    }
}

/// Generates one `#[test]` per file listed in a suite manifest.
///
/// ```ignore
/// tpt_testkit::suite::tpt_suite_tests!("tests/suites/unicode-bidi.json", check = check_bidi);
///
/// fn check_bidi(bytes: &[u8]) {
///     assert!(!bytes.is_empty());
/// }
/// ```
///
/// The manifest path is relative to the crate's `CARGO_MANIFEST_DIR`. Each test name is
/// `suite_` followed by the entry path with non-alphanumeric characters replaced by `_`. Each
/// test resolves its file through `tpt_testkit::suite::ensure_file` and then calls the check
/// function with the file contents.
///
/// The macro reads the manifest when it expands. The generated code also has an `include_str!`
/// of the manifest, so editing the manifest makes Cargo rebuild the crate.
#[proc_macro]
pub fn tpt_suite_tests(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as SuiteTests);
    match expand(&args) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand(args: &SuiteTests) -> syn::Result<proc_macro2::TokenStream> {
    let relative = args.manifest.value();
    let span = args.manifest.span();

    let root = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|err| syn::Error::new(span, format!("CARGO_MANIFEST_DIR is not set: {err}")))?;
    let full = PathBuf::from(&root).join(&relative);
    let text = std::fs::read_to_string(&full)
        .map_err(|err| syn::Error::new(span, format!("cannot read {}: {err}", full.display())))?;
    let manifest: ManifestFile = serde_json::from_str(&text)
        .map_err(|err| syn::Error::new(span, format!("invalid suite manifest: {err}")))?;

    let check = &args.check;
    let mut names = BTreeSet::new();
    let mut tests = Vec::with_capacity(manifest.files.len());
    for entry in &manifest.files {
        let name = format!("suite_{}", sanitize(&entry.path));
        if !names.insert(name.clone()) {
            return Err(syn::Error::new(
                span,
                format!("two suite entries produce the test name `{name}`"),
            ));
        }
        let ident = Ident::new(&name, Span::call_site());
        let path = &entry.path;
        tests.push(quote! {
            #[test]
            fn #ident() {
                let manifest = ::tpt_testkit::suite::Manifest::from_json(
                    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #relative))
                )
                .expect("suite manifest is valid");
                let options = ::tpt_testkit::suite::Options::from_env();
                let file = ::tpt_testkit::suite::ensure_file(
                    &manifest,
                    #path,
                    &options,
                    ::tpt_testkit::suite::fetch_with_curl,
                )
                .unwrap_or_else(|err| panic!("{err}"));
                let bytes = ::std::fs::read(&file).expect("cached suite file is readable");
                #check(&bytes);
            }
        });
    }

    Ok(quote! {
        const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #relative));
        #(#tests)*
    })
}

fn sanitize(path: &str) -> String {
    path.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}
