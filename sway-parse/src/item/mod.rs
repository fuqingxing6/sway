use crate::{Parse, ParseResult, ParseToEnd, Parser, ParserConsumed};

use sway_ast::keywords::{
    AbiToken, ClassToken, ConfigurableToken, ConstToken, DepToken, EnumToken, FnToken, ImplToken,
    MutToken, OpenAngleBracketToken, OpenCurlyBraceToken, RefToken, SelfToken, SemicolonToken,
    StorageToken, StructToken, TraitToken, UseToken, WhereToken,
};
use sway_ast::{
    Dependency, FnArg, FnArgs, FnReturnType, FnSignature, ImplicitReturn, ItemConst, ItemEnum,
    ItemFn, ItemKind, ItemStruct, ItemTrait, ItemUse, TypeField,
};
use sway_error::parser_error::ParseErrorKind;
use sway_types::Spanned;

mod item_abi;
mod item_configurable;
mod item_const;
mod item_enum;
mod item_fn;
mod item_impl;
mod item_storage;
mod item_struct;
mod item_trait;
mod item_use;

impl Parse for ItemKind {
    fn parse(parser: &mut Parser) -> ParseResult<ItemKind> {
        // FIXME(Centril): Visibility should be moved out of `ItemKind` variants,
        // introducing a struct `Item` that holds the visibility and the kind,
        // and then validate in an "AST validation" step which kinds that should have `pub`s.

        let mut visibility = parser.take();

        let kind = if let Some(item) = parser.guarded_parse::<DepToken, Dependency>()? {
            ItemKind::Dependency(item)
        } else if let Some(mut item) = parser.guarded_parse::<UseToken, ItemUse>()? {
            item.visibility = visibility.take();
            ItemKind::Use(item)
        } else if let Some(mut item) = parser.guarded_parse::<ClassToken, ItemStruct>()? {
            item.visibility = visibility.take();
            ItemKind::Struct(item)
        } else if let Some(mut item) = parser.guarded_parse::<StructToken, ItemStruct>()? {
            item.visibility = visibility.take();
            ItemKind::Struct(item)
        } else if let Some(mut item) = parser.guarded_parse::<EnumToken, ItemEnum>()? {
            item.visibility = visibility.take();
            ItemKind::Enum(item)
        } else if let Some(mut item) = parser.guarded_parse::<FnToken, ItemFn>()? {
            item.fn_signature.visibility = visibility.take();
            ItemKind::Fn(item)
        } else if let Some(mut item) = parser.guarded_parse::<TraitToken, ItemTrait>()? {
            item.visibility = visibility.take();
            ItemKind::Trait(item)
        } else if let Some(item) = parser.guarded_parse::<ImplToken, _>()? {
            ItemKind::Impl(item)
        } else if let Some(item) = parser.guarded_parse::<AbiToken, _>()? {
            ItemKind::Abi(item)
        } else if let Some(mut item) = parser.guarded_parse::<ConstToken, ItemConst>()? {
            item.visibility = visibility.take();
            parser.take::<SemicolonToken>();
            ItemKind::Const(item)
        } else if let Some(item) = parser.guarded_parse::<StorageToken, _>()? {
            ItemKind::Storage(item)
        } else if let Some(item) = parser.guarded_parse::<ConfigurableToken, _>()? {
            ItemKind::Configurable(item)
        } else {
            return Err(parser.emit_error(ParseErrorKind::ExpectedAnItem));
        };

        // Ban visibility qualifiers that haven't been consumed, but do so with recovery.
        let _ = parser.ban_visibility_qualifier(&visibility);

        Ok(kind)
    }
}

impl Parse for TypeField {
    fn parse(parser: &mut Parser) -> ParseResult<TypeField> {
        Ok(TypeField {
            name: parser.parse()?,
            colon_token: parser.parse()?,
            ty: parser.parse()?,
        })
    }
}

impl ParseToEnd for FnArgs {
    fn parse_to_end<'a, 'e>(
        mut parser: Parser<'a, '_>,
    ) -> ParseResult<(FnArgs, ParserConsumed<'a>)> {
        let mut ref_self: Option<RefToken> = None;
        let mut mutable_self: Option<MutToken> = None;
        if parser.peek::<(MutToken, SelfToken)>().is_some()
            || parser.peek::<(RefToken, MutToken, SelfToken)>().is_some()
        {
            ref_self = parser.take();
            mutable_self = parser.take();
        }
        match parser.take() {
            Some(self_token) => {
                match parser.take() {
                    Some(comma_token) => {
                        let (args, consumed) = parser.parse_to_end()?;
                        let fn_args = FnArgs::NonStatic {
                            self_token,
                            ref_self,
                            mutable_self,
                            args_opt: Some((comma_token, args)),
                        };
                        Ok((fn_args, consumed))
                    }
                    None => {
                        let fn_args = FnArgs::NonStatic {
                            self_token,
                            ref_self,
                            mutable_self,
                            args_opt: None,
                        };
                        match parser.check_empty() {
                            Some(consumed) => Ok((fn_args, consumed)),
                            None => Err(parser
                                .emit_error(ParseErrorKind::ExpectedCommaOrCloseParenInFnArgs)),
                        }
                    }
                }
            }
            None => {
                let (args, consumed) = parser.parse_to_end()?;
                let fn_args = FnArgs::Static(args);
                Ok((fn_args, consumed))
            }
        }
    }
}

impl Parse for FnArg {
    fn parse(parser: &mut Parser) -> ParseResult<FnArg> {
        Ok(FnArg {
            pattern: parser.parse()?,
            colon_token: parser.parse()?,
            ty: parser.parse()?,
        })
    }
}

impl Parse for FnSignature {
    fn parse(parser: &mut Parser) -> ParseResult<FnSignature> {
        let visibility = parser.take();
        let fn_token = parser.parse()?;
        let name = parser.parse()?;
        let generics = parser.guarded_parse::<OpenAngleBracketToken, _>()?;
        let arguments = parser.parse()?;
        let mut where_clause_opt = None;
        let return_type = match parser.take() {
            Some(right_arrow_token) => {
                let ty = parser.parse()?;
                where_clause_opt = parser.guarded_parse::<WhereToken, _>()?;
                FnReturnType::TypedReturn((right_arrow_token, ty))
            }
            None => {
                where_clause_opt = parser.guarded_parse::<WhereToken, _>()?;
                let Some(open_curly_brace) = parser.peek::<OpenCurlyBraceToken>();
                FnReturnType::Implicit(ImplicitReturn {
                    span: open_curly_brace.span(),
                })
            }
        };
        Ok(FnSignature {
            visibility,
            fn_token,
            name,
            generics,
            arguments,
            return_type,
            where_clause_opt,
        })
    }
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use sway_ast::{AttributeDecl, Item, ItemTraitItem};

    fn parse_item(input: &str) -> Item {
        let handler = <_>::default();
        let ts = crate::token::lex(&handler, &Arc::from(input), 0, input.len(), None).unwrap();
        Parser::new(&handler, &ts)
            .parse()
            .unwrap_or_else(|_| panic!("Parse error: {:?}", handler.consume().0))
    }

    // Attribute name and its list of parameters
    type ParameterizedAttr<'a> = (&'a str, Option<Vec<&'a str>>);

    fn attributes(attribute_list: &[AttributeDecl]) -> Vec<Vec<ParameterizedAttr>> {
        attribute_list
            .iter()
            .map(|attr_decl| {
                attr_decl
                    .attribute
                    .get()
                    .into_iter()
                    .map(|att| {
                        (
                            att.name.as_str(),
                            att.args
                                .as_ref()
                                .map(|arg| arg.get().into_iter().map(|a| a.as_str()).collect()),
                        )
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn parse_doc_comment() {
        let item = parse_item(
            r#"
            // I will be ignored.
            //! I will be ignored.
            /// This is a doc comment.
            //! I will be ignored.
            // I will be ignored.
            fn f() -> bool {
                false
            }
            "#,
        );
        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("doc-comment", Some(vec![" This is a doc comment."]))]]
        );
    }

    #[test]
    fn parse_doc_comment_struct() {
        let item = parse_item(
            r#"
            // I will be ignored.
            //! I will be ignored. 
            /// This is a doc comment.
            //! I will be ignored.
            // I will be ignored.
            struct MyStruct {
                // I will be ignored.
                //! I will be ignored.
                /// This is a doc comment.
                //! I will be ignored.
                // I will be ignored.
                a: bool,
            }
            "#,
        );

        /* struct annotations */
        assert!(matches!(item.value, ItemKind::Struct(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("doc-comment", Some(vec![" This is a doc comment."]))]]
        );

        /* struct field annotations */
        let item = match item.value {
            ItemKind::Struct(item) => item.fields.inner.into_iter().next().unwrap(),
            _ => unreachable!(),
        };

        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("doc-comment", Some(vec![" This is a doc comment."]))]]
        );
    }

    #[test]
    fn parse_attributes_none() {
        let item = parse_item(
            r#"
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert!(item.attribute_list.is_empty());
    }

    #[test]
    fn parse_attributes_fn_basic() {
        let item = parse_item(
            r#"
            #[foo]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(attributes(&item.attribute_list), vec![[("foo", None)]]);
    }

    #[test]
    fn parse_attributes_fn_two_basic() {
        let item = parse_item(
            r#"
            #[foo]
            #[bar]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));

        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("foo", None)], [("bar", None)]]
        );
    }

    #[test]
    fn parse_attributes_fn_one_arg() {
        let item = parse_item(
            r#"
            #[foo(one)]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("foo", Some(vec!["one"]))]]
        );
    }

    #[test]
    fn parse_attributes_fn_empty_parens() {
        let item = parse_item(
            r#"
            #[foo()]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("foo", Some(vec![]))]]
        );
    }

    #[test]
    fn parse_attributes_fn_zero_and_one_arg() {
        let item = parse_item(
            r#"
            #[bar]
            #[foo(one)]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("bar", None)], [("foo", Some(vec!["one"]))]]
        );
    }

    #[test]
    fn parse_attributes_fn_one_and_zero_arg() {
        let item = parse_item(
            r#"
            #[foo(one)]
            #[bar]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("foo", Some(vec!["one"]))], [("bar", None)]]
        );
    }

    #[test]
    fn parse_attributes_fn_two_args() {
        let item = parse_item(
            r#"
            #[foo(one, two)]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![[("foo", Some(vec!["one", "two"]))]]
        );
    }

    #[test]
    fn parse_attributes_fn_zero_one_and_three_args() {
        let item = parse_item(
            r#"
            #[bar]
            #[foo(one)]
            #[baz(two,three,four)]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![
                [("bar", None)],
                [("foo", Some(vec!["one"]))],
                [("baz", Some(vec!["two", "three", "four"]))]
            ]
        );
    }

    #[test]
    fn parse_attributes_fn_zero_one_and_three_args_in_one_attribute_decl() {
        let item = parse_item(
            r#"
            #[bar, foo(one), baz(two,three,four)]
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![[
                ("bar", None),
                ("foo", Some(vec!["one"])),
                ("baz", Some(vec!["two", "three", "four"]))
            ]]
        );
    }

    #[test]
    fn parse_attributes_trait() {
        let item = parse_item(
            r#"
            trait T {
                #[foo(one)]
                #[bar]
                fn f() -> bool;
            } {
                #[bar(one, two, three)]
                fn g() -> bool {
                    f()
                }
            }
            "#,
        );

        // The trait itself has no attributes.
        assert!(matches!(item.value, ItemKind::Trait(_)));
        assert_eq!(item.attribute_list.len(), 0);

        if let ItemKind::Trait(item_trait) = item.value {
            let mut decls = item_trait.trait_items.get().iter();

            let trait_item = decls.next();
            assert!(trait_item.is_some());
            let (annotated, _) = trait_item.unwrap();
            #[allow(irrefutable_let_patterns)]
            if let ItemTraitItem::Fn(_fn_sig) = &annotated.value {
                assert_eq!(
                    attributes(&annotated.attribute_list),
                    vec![[("foo", Some(vec!["one"]))], [("bar", None)]]
                );
            }

            assert!(decls.next().is_none());

            assert!(item_trait.trait_defs_opt.is_some());
            let mut defs = item_trait.trait_defs_opt.as_ref().unwrap().get().iter();

            let g_sig = defs.next();
            assert!(g_sig.is_some());

            assert_eq!(
                attributes(&g_sig.unwrap().attribute_list),
                vec![[("bar", Some(vec!["one", "two", "three"]))],]
            );

            assert!(defs.next().is_none());
        } else {
            panic!("Parsed trait is not a trait.");
        }
    }

    #[test]
    fn parse_attributes_abi() {
        let item = parse_item(
            r#"
            abi A {
                #[bar(one, two, three)]
                fn f() -> bool;

                #[foo]
                fn g() -> u64;
            } {
                #[baz(one)]
                fn h() -> bool {
                    f()
                }
            }
            "#,
        );

        // The ABI itself has no attributes.
        assert!(matches!(item.value, ItemKind::Abi(_)));
        assert_eq!(item.attribute_list.len(), 0);

        if let ItemKind::Abi(item_abi) = item.value {
            let mut decls = item_abi.abi_items.get().iter();

            let f_sig = decls.next();
            assert!(f_sig.is_some());

            assert_eq!(
                attributes(&f_sig.unwrap().0.attribute_list),
                vec![[("bar", Some(vec!["one", "two", "three"]))],]
            );

            let g_sig = decls.next();
            assert!(g_sig.is_some());

            assert_eq!(
                attributes(&g_sig.unwrap().0.attribute_list),
                vec![[("foo", None)],]
            );
            assert!(decls.next().is_none());

            assert!(item_abi.abi_defs_opt.is_some());
            let mut defs = item_abi.abi_defs_opt.as_ref().unwrap().get().iter();

            let h_sig = defs.next();
            assert!(h_sig.is_some());

            assert_eq!(
                attributes(&h_sig.unwrap().attribute_list),
                vec![[("baz", Some(vec!["one"]))],]
            );
            assert!(defs.next().is_none());
        } else {
            panic!("Parsed ABI is not an ABI.");
        }
    }

    #[test]
    fn parse_attributes_doc_comment() {
        let item = parse_item(
            r#"
            /// This is a doc comment.
            /// This is another doc comment.
            fn f() -> bool {
                false
            }
            "#,
        );

        assert!(matches!(item.value, ItemKind::Fn(_)));
        assert_eq!(
            attributes(&item.attribute_list),
            vec![
                [("doc-comment", Some(vec![" This is a doc comment."]))],
                [("doc-comment", Some(vec![" This is another doc comment."]))]
            ]
        );
    }
}
