use crate::ArcDpsGen;
use syn::{
    Error, Expr, FieldValue, Lit, Member, Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// Helper to generate parsing.
macro_rules! match_parse {
    ($ident:expr, $generator:expr, $field:expr, $($name:ident),+; extras: { $($extras:ident),+ }) => {
        paste::paste! {
            match $ident.to_string().as_str() {
                $(
                    stringify!([<raw_ $name>]) => {
                        $generator.[<raw_ $name>] = Some($field.expr);
                        if $generator.$name.is_some() {
                            return Err(Error::new_spanned(
                                $ident,
                                stringify!([<raw_ $name>] and $name are exclusive),
                            ));
                        }
                    }
                    stringify!($name) => {
                        $generator.$name = Some($field.expr);
                        if $generator.[<raw_ $name>].is_some() {
                            return Err(Error::new_spanned(
                                $ident,
                                stringify!($name and [<raw_ $name>] are exclusive),
                            ));
                        }
                    }
                )+
                $(
                    #[cfg(feature = "extras")]
                    stringify!([<raw_ $extras>]) => {
                        $generator.extras.[<raw_ $extras>] = Some($field.expr);
                        if $generator.extras.$extras.is_some() {
                            return Err(Error::new_spanned(
                                $ident,
                                stringify!([<raw_ $extras>] and $extras are exclusive),
                            ));
                        }
                    }
                    #[cfg(feature = "extras")]
                    stringify!($extras) => {
                        $generator.extras.$extras = Some($field.expr);
                        if $generator.extras.[<raw_ $extras>].is_some() {
                            return Err(Error::new_spanned(
                                $ident,
                                stringify!($extras and [<raw_ $extras>] are exclusive),
                            ));
                        }
                    }
                    #[cfg(not(feature = "extras"))]
                    stringify!([<raw_ $extras>]) | stringify!($extras) => {
                        return Err(Error::new_spanned(
                            $ident,
                            format!("field {} requires the extras feature", $ident),
                        ));
                    }
                )+
                _ => return Err(Error::new_spanned(
                    $ident,
                    format!("no field named {} exists", $ident),
                )),
            }
        }
    }
}

impl Parse for ArcDpsGen {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fields: Punctuated<FieldValue, Token![,]> = Punctuated::parse_terminated(input)?;

        let mut generator = Self::default();
        let mut sig_done = false;

        for field in fields.into_iter() {
            if let Member::Named(ident) = &field.member {
                match ident.to_string().as_str() {
                    "name" => {
                        generator.name = if let Expr::Lit(expr) = field.expr {
                            if let Lit::Str(lit) = expr.lit {
                                Some(lit)
                            } else {
                                return Err(Error::new_spanned(
                                    expr,
                                    "name needs to be a literal of type &'static str",
                                ));
                            }
                        } else {
                            return Err(Error::new_spanned(
                                field.expr,
                                "name needs to be a literal of type &'static str",
                            ));
                        };
                    }
                    "sig" => {
                        sig_done = true;
                        generator.sig = field.expr;
                    }

                    "init" => generator.init = Some(field.expr),
                    "release" => generator.release = Some(field.expr),
                    "update_url" => generator.update_url = Some(field.expr),

                    _ => {
                        match_parse!(
                            ident,
                            generator,
                            field,
                            combat,
                            combat_local,
                            imgui,
                            options_end,
                            options_windows,
                            wnd_filter,
                            wnd_nofilter;
                            extras: {
                                extras_init,
                                extras_squad_update,
                                extras_language_changed,
                                extras_keybind_changed,
                                extras_squad_chat_message,
                                extras_chat_message
                            }
                        )
                    }
                };
            } else {
                return Err(Error::new_spanned(&field.member, "field must have a name"));
            }
        }

        if !sig_done {
            return Err(Error::new(input.span(), "sig field is required"));
        }

        Ok(generator)
    }
}
