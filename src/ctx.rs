//! What the core needs to ask the front-end.
//!
//! Everything in this crate works on machine-neutral IR; the few questions
//! that depend on the machine's *class metadata* — how nested classes are
//! related, what a method's signature says, how a nested method body looks —
//! go through [`Ctx`]. A front-end implements it once (JVM: over its class
//! pool; DEX: over its class definitions) and every shared pass/emitter works.
//!
//! The trait is deliberately *semantic*, not a raw pool: the core never sees
//! a constant pool, a `Signature` attribute, or an `InnerClasses` table.

use std::collections::{HashMap, HashSet};

use crate::ir::expr::Expr;
use crate::ir::expr::TypeRef;
use crate::ir::stmt::Stmt;
use crate::types::{ClassAccessFlags, MethodDescriptor};

/// How a nested class was classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NestedKind {
    /// Declared as a member of its outer class.
    Member,
    /// An anonymous class: inlined at its `new` site, never printed separately.
    Anonymous,
    /// A local class: declared at its use site inside a method.
    Local,
    /// A lambda implementation class: skipped entirely.
    Lambda,
}

/// One nested class of a family.
#[derive(Debug, Clone)]
pub struct NestedClass {
    /// Internal name (e.g. `p/Outer$Inner`).
    pub name: String,
    /// Source-visible simple name (`Inner`, or a synthesized name for locals).
    pub simple: String,
    pub kind: NestedKind,
    pub access: ClassAccessFlags,
    /// Source header rendered from the class's generic signature when the
    /// container records one; `None` for front-ends without signature info
    /// (DEX), in which case tracking falls back to the erased rendering.
    pub sig_header: Option<String>,
}

/// The nested-class family of one top-level class.
#[derive(Debug, Default, Clone)]
pub struct Family {
    pub root: String,
    /// All nested classes (any depth) keyed by internal name.
    pub nested: HashMap<String, NestedClass>,
    /// Anonymous classes inlined at `new` sites.
    pub anonymous: HashSet<String>,
    /// Local classes declared at their use site.
    pub locals: HashSet<String>,
    /// Lambda implementation classes (skipped).
    pub lambdas: HashSet<String>,
}

/// A decompiled nested method body (lambda body, anonymous-class method, ...).
pub struct MethodBody {
    /// Final statement tree.
    pub body: Stmt,
    pub vt: crate::var::VarTable,
    pub desc: MethodDescriptor,
}

/// Front-end services used by the shared passes and the emitter.
pub trait Ctx {
    /// Internal name of the class currently being emitted
    /// (e.g. `java/util/ArrayList`).
    fn class_name(&self) -> &str;

    /// Source level gate for version-dependent rendering: the JVM front-end
    /// returns the class-file major version; a DEX front-end returns the
    /// equivalent language level it decided to target. `52` means "Java 8
    /// features available" (lambdas, diamond, ...).
    fn source_level(&self) -> u16 {
        52
    }

    /// Identity of the backing container (pool), for caches keyed on
    /// pool-dependent pure queries (`Printer::shorten` memoization).
    /// Default 0 is safe only for single-pool processes; multi-pool
    /// front-ends (ddc/jcdc) override with the pool pointer so a second
    /// pool never reads the first pool's cached shortenings.
    fn pool_id(&self) -> u64 {
        0
    }

    /// The class that `internal` is nested in, when the container records the
    /// nesting (JVM `InnerClasses`/`NestHost`; DEX inner-class annotations).
    /// `None` for a top-level class — including literal-`$` names that only
    /// *look* nested.
    fn find_outer(&self, internal: &str) -> Option<String>;

    /// Nested-class family of `root`, as far as the container records it.
    fn family(&self, root: &str) -> Family;

    /// True when a nested class is static (`Outer$Inner` with ACC_STATIC), so
    /// it is not constructed with an enclosing instance.
    fn nested_is_static(&self, internal: &str) -> bool;

    /// True when the class carries a synthetic `this$0` field (an inner class
    /// with an enclosing instance parameter).
    fn class_has_this0(&self, internal: &str) -> bool;

    /// True when `sub` is assignable to `sup` (both internal names).
    fn is_subtype_of(&self, sub: &crate::types::JavaType, sup: &str) -> bool;

    /// Superclasses/interfaces of `internal` with the given type arguments
    /// applied: `[(java/lang/Object, []), ...]`, most-derived first.
    fn class_supers_args(
        &self,
        internal: &str,
        args: &[crate::types::GenericType],
    ) -> Vec<(String, Vec<crate::types::GenericType>)>;

    /// True when the call carries inferred type arguments that the source
    /// must spell out.
    fn is_generic_call(&self, e: &Expr) -> bool;

    /// Instantiated formal parameter types of a generic call plus the callee's
    /// own method type variables (for witness casts). `None` when the call has
    /// no inferred type arguments to spell out.
    fn generic_call_formals(
        &self,
        e: &Expr,
    ) -> Option<(Vec<crate::types::GenericType>, Vec<String>)>;

    /// The instantiated return type of a call whose methodref owner redeclares
    /// the method with a self-covariant return (Stream over BaseStream).
    fn polymorphic_ret_cast(
        &self,
        cls: &str,
        name: &str,
        desc: &MethodDescriptor,
    ) -> Option<crate::types::JavaType>;

    /// Constructor parameter types of `internal` matching `arity`, for
    /// rendering `new` arguments against the right overload.
    fn ctor_formals_by_arity(
        &self,
        internal: &str,
        arity: usize,
    ) -> Option<Vec<crate::types::GenericType>>;

    /// Local-class name recorded for a `$`-style synthetic marker, when the
    /// container records one (`Outer$1Local` -> `Local`).
    fn local_class_internal(&self, simple: &str) -> Option<String> {
        let _ = simple;
        None
    }

    /// Generic type parameters declared by `internal` (from its class
    /// signature); empty when the container records none.
    fn class_type_params(&self, internal: &str) -> Vec<crate::types::TypeParam> {
        let _ = internal;
        Vec::new()
    }

    /// Type variables of the *method* the call refers to (used to decide
    /// whether a formal still carries the callee's own typevars).
    fn generic_call_mt_typevars(&self, e: &Expr) -> Vec<String> {
        let _ = e;
        Vec::new()
    }

    /// Superclass of `internal` (internal name); `None` at `Object`/unknown.
    fn super_name(&self, internal: &str) -> Option<String> {
        let _ = internal;
        None
    }

    /// Access flags of the field `internal.name`; `None` when absent.
    fn field_flags(&self, internal: &str, name: &str) -> Option<crate::types::FieldAccessFlags> {
        let _ = (internal, name);
        None
    }

    /// Access flags of the method `internal.name(desc)`; `None` when absent.
    fn method_flags(
        &self,
        internal: &str,
        name: &str,
        desc: &str,
    ) -> Option<crate::types::MethodAccessFlags> {
        let _ = (internal, name, desc);
        None
    }

    /// True when `internal` declares any field named `name`.
    fn declares_field(&self, internal: &str, name: &str) -> bool {
        self.field_flags(internal, name).is_some()
    }

    /// True when `internal` declares a method named `name` (any descriptor).
    fn declares_method_named(&self, internal: &str, name: &str) -> bool {
        let _ = (internal, name);
        false
    }

    /// True when `internal` is an interface.
    fn is_interface(&self, internal: &str) -> bool;

    /// True when `internal` is sealed (its permitted subclasses are recorded).
    fn is_sealed(&self, internal: &str) -> bool;

    /// True when the container holds this class at all (naming probes use
    /// it to test `Outer$1Local`-style candidates).
    fn has_class(&self, internal: &str) -> bool;

    /// Interfaces and superclass of `internal` (internal names, declaration
    /// order). `None` when the class is unknown to the front-end.
    fn class_bases(&self, internal: &str) -> Option<(Vec<String>, Option<String>)>;

    /// True when `internal` is an inner class whose constructor forwards its
    /// enclosing instance straight to `super` (javac 21+ drops `this$0` in
    /// that case, but call sites still need the qualified-new form).
    fn outer_param_via_super(&self, internal: &str) -> bool;

    /// Decompile the nested method a lambda site refers to — a lambda body,
    /// an anonymous-class method, or a local-class method — and run the
    /// front-end's own idiom recovery on it.
    ///
    /// This is where machine-specific desugaring lives: the shared emitter
    /// asks for a finished statement tree and prints it. The `LambdaExpr`
    /// carries the call-site facts (impl owner/name/descriptor, the SAM's
    /// parameter names, the capture expressions, and any front-end
    /// annotations it attached, e.g. capture snapshots); `outer_vt` is the
    /// enclosing method's variable table — the lexical scope the body will be
    /// printed in.
    fn nested_method(
        &self,
        l: &crate::ir::expr::LambdaExpr,
        outer_vt: &crate::var::VarTable,
    ) -> Option<MethodBody>;

    /// Parameter types of the `<init>` of `internal` matching `skip + n`
    /// descriptor args, with `skip` leading synthetic parameters removed.
    /// Used to print `new` arguments with boolean/char constant adjustment.
    fn ctor_param_types(
        &self,
        internal: &str,
        skip: usize,
        n: usize,
        args: &[Expr],
    ) -> Option<Vec<crate::types::JavaType>> {
        let _ = (internal, skip, n, args);
        None
    }

    /// Instantiated SAM return of a functional interface's method, resolved
    /// against a cast's type arguments (`(Function<I,R>) lambda` needs
    /// `(R) value` witnesses on the impl body's returns).
    fn sam_ret_cast(&self, g: &crate::types::GenericType, sam_name: &str) -> Option<TypeRef> {
        let _ = (g, sam_name);
        None
    }

    /// True when `internal` declares class-level type parameters (so
    /// `new internal<>(...)` should carry a diamond).
    fn class_declares_generics(&self, internal: &str) -> bool {
        !self.class_type_params(internal).is_empty()
    }
}

/// A `Ctx` with no metadata: every query answers "unknown".
///
/// Useful for tests and for front-ends that have not implemented a query yet —
/// the core degrades gracefully (no nesting assumptions, no generic casts).
pub struct NullCtx {
    pub class_name: String,
    pub level: u16,
}

impl Default for NullCtx {
    fn default() -> Self {
        NullCtx {
            class_name: String::new(),
            level: 52,
        }
    }
}

impl Ctx for NullCtx {
    fn class_name(&self) -> &str {
        &self.class_name
    }
    fn source_level(&self) -> u16 {
        self.level
    }
    fn find_outer(&self, _internal: &str) -> Option<String> {
        None
    }
    fn family(&self, root: &str) -> Family {
        Family {
            root: root.to_string(),
            ..Default::default()
        }
    }
    fn nested_is_static(&self, _internal: &str) -> bool {
        true
    }
    fn class_has_this0(&self, _internal: &str) -> bool {
        false
    }
    fn is_subtype_of(&self, _sub: &crate::types::JavaType, _sup: &str) -> bool {
        false
    }
    fn class_supers_args(
        &self,
        _internal: &str,
        _args: &[crate::types::GenericType],
    ) -> Vec<(String, Vec<crate::types::GenericType>)> {
        Vec::new()
    }
    fn is_generic_call(&self, _e: &Expr) -> bool {
        false
    }
    fn generic_call_formals(
        &self,
        _e: &Expr,
    ) -> Option<(Vec<crate::types::GenericType>, Vec<String>)> {
        None
    }
    fn polymorphic_ret_cast(
        &self,
        _cls: &str,
        _name: &str,
        _desc: &MethodDescriptor,
    ) -> Option<crate::types::JavaType> {
        None
    }
    fn ctor_formals_by_arity(
        &self,
        _internal: &str,
        _arity: usize,
    ) -> Option<Vec<crate::types::GenericType>> {
        None
    }
    fn is_interface(&self, _internal: &str) -> bool {
        false
    }
    fn is_sealed(&self, _internal: &str) -> bool {
        false
    }
    fn has_class(&self, _internal: &str) -> bool {
        false
    }
    fn class_bases(&self, _internal: &str) -> Option<(Vec<String>, Option<String>)> {
        None
    }
    fn outer_param_via_super(&self, _internal: &str) -> bool {
        false
    }
    fn nested_method(
        &self,
        _l: &crate::ir::expr::LambdaExpr,
        _outer_vt: &crate::var::VarTable,
    ) -> Option<MethodBody> {
        None
    }
}

/// Assemble a [`MethodBody`] (front-ends return these from `Ctx::nested_method`).
pub fn ctx_method_body(
    body: crate::ir::stmt::Stmt,
    vt: crate::var::VarTable,
    desc: MethodDescriptor,
) -> MethodBody {
    MethodBody { body, vt, desc }
}
