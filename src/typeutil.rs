//! Pure helpers over the generic type model.
//!
//! These are container-independent: they only read [`crate::types`] values, so
//! every front-end (JVM signatures, DEX-inferred types) shares them.

/// Internal name of a `ClassSig` (`java/util/List$Node`).
pub fn classsig_internal(cs: &crate::types::ClassSig) -> String {
    let joined = cs
        .parts
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join("$");
    if cs.package.is_empty() {
        joined
    } else {
        format!("{}/{}", cs.package, joined)
    }
}

pub fn subst_typevars(
    t: &crate::types::GenericType,
    params: &[crate::types::TypeParam],
    args: &[crate::types::GenericType],
) -> crate::types::GenericType {
    use crate::types::GenericType as G;
    match t {
        G::TypeVar(n) => params
            .iter()
            .position(|p| &p.name == n)
            .and_then(|i| args.get(i).cloned())
            .unwrap_or_else(|| t.clone()),
        G::Array(i) => G::Array(Box::new(subst_typevars(i, params, args))),
        G::Class(cs) => G::Class(crate::types::ClassSig {
            package: cs.package.clone(),
            parts: cs
                .parts
                .iter()
                .map(|p| crate::types::ClassSigPart {
                    name: p.name.clone(),
                    args: p
                        .args
                        .iter()
                        .map(|a| subst_typevars(a, params, args))
                        .collect(),
                })
                .collect(),
        }),
        G::Wildcard(crate::types::WildcardBound::Extends(i)) => G::Wildcard(
            crate::types::WildcardBound::Extends(Box::new(subst_typevars(i, params, args))),
        ),
        G::Wildcard(crate::types::WildcardBound::Super(i)) => G::Wildcard(
            crate::types::WildcardBound::Super(Box::new(subst_typevars(i, params, args))),
        ),
        other => other.clone(),
    }
}

pub fn g_has_typevar(g: &crate::types::GenericType) -> bool {
    use crate::types::GenericType as G;
    match g {
        G::TypeVar(_) => true,
        G::Array(i) => g_has_typevar(i),
        G::Class(cs) => cs.parts.iter().any(|p| p.args.iter().any(g_has_typevar)),
        G::Wildcard(w) => match w {
            crate::types::WildcardBound::Extends(i) | crate::types::WildcardBound::Super(i) => {
                g_has_typevar(i)
            }
            _ => false,
        },
        _ => false,
    }
}

pub fn g_has_wildcard(g: &crate::types::GenericType) -> bool {
    use crate::types::GenericType as G;
    match g {
        G::Wildcard(_) => true,
        G::Array(i) => g_has_wildcard(i),
        G::Class(cs) => cs.parts.iter().any(|p| p.args.iter().any(g_has_wildcard)),
        _ => false,
    }
}

pub fn g_mentions_any(g: &crate::types::GenericType, names: &[String]) -> bool {
    g_mentions_any_inner(g, names)
}

fn g_mentions_any_inner(g: &crate::types::GenericType, names: &[String]) -> bool {
    use crate::types::GenericType as G;
    match g {
        G::TypeVar(n) => names.iter().any(|x| x == n),
        G::Array(i) => g_mentions_any_inner(i, names),
        G::Class(cs) => cs
            .parts
            .iter()
            .any(|p| p.args.iter().any(|a| g_mentions_any_inner(a, names))),
        G::Wildcard(crate::types::WildcardBound::Extends(t))
        | G::Wildcard(crate::types::WildcardBound::Super(t)) => g_mentions_any_inner(t, names),
        _ => false,
    }
}
