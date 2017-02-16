// Copyright 2016 Kyle Mayes
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A somewhat idiomatic Rust wrapper for libclang.
//!
//! ## Supported Versions
//!
//! * 3.5 - [Documentation](https://kylemayes.github.io/clang-rs/3_5/clang)
//! * 3.6 - [Documentation](https://kylemayes.github.io/clang-rs/3_6/clang)
//! * 3.7 - [Documentation](https://kylemayes.github.io/clang-rs/3_7/clang)
//! * 3.8 - [Documentation](https://kylemayes.github.io/clang-rs/3_8/clang)
//! * 3.9 - [Documentation](https://kylemayes.github.io/clang-rs/3_9/clang)

#![warn(missing_copy_implementations, missing_debug_implementations, missing_docs)]

#![allow(non_upper_case_globals)]

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![cfg_attr(feature="clippy", warn(clippy))]

#[macro_use]
extern crate lazy_static;

extern crate clang_sys;
extern crate libc;

#[macro_use]
mod utility;

pub mod completion;
pub mod diagnostic;
pub mod documentation;
pub mod source;
pub mod token;

pub mod sonar;

use std::cmp;
use std::fmt;
use std::hash;
use std::mem;
use std::ptr;
use std::slice;
use std::collections::{HashMap};
use std::ffi::{CString};
use std::marker::{PhantomData};
use std::path::{Path, PathBuf};
use std::sync::atomic::{self, AtomicBool};

use clang_sys::*;

use libc::{c_int, c_uint, c_ulong};

use completion::{Completer, CompletionString};
use diagnostic::{Diagnostic};
use documentation::{Comment};
use source::{File, Module, SourceLocation, SourceRange};
use token::{Token};
use utility::{FromError, Nullable};

mod error;
pub use self::error::*;

//================================================
// Enums
//================================================

// Accessibility _________________________________

/// Indicates the accessibility of a declaration or base class specifier.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum Accessibility {
    /// The declaration or base class specifier is private.
    Private = 3,
    /// The declaration or base class specifier is protected.
    Protected = 2,
    /// The declaration or base class specifier is public.
    Public = 1,
}

// Availability __________________________________

/// Indicates the availability of an AST entity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum Availability {
    /// The entity is available.
    Available = 0,
    /// The entity is available but has been deprecated and any usage of it will be a warning.
    Deprecated = 1,
    /// The entity is available but is not accessible and any usage of it will be an error.
    Inaccessible = 3,
    /// The entity is not available and any usage of it will be an error.
    Unavailable = 2,
}

// CallingConvention _____________________________

/// Indicates the calling convention specified for a function type.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum CallingConvention {
    /// The function type uses a calling convention that is not exposed via this interface.
    Unexposed = 200,
    /// The function type uses the x86 `cdecl` calling convention.
    Cdecl = 1,
    /// The function type uses the x86 `fastcall` calling convention.
    Fastcall = 3,
    /// The function type uses the x86 `pascal` calling convention.
    Pascal = 5,
    /// The function type uses the x86 `stdcall` calling convention.
    Stdcall = 2,
    /// The function type uses the x86 `thiscall` calling convention.
    Thiscall = 4,
    /// The function type uses the x86 `vectorcall` calling convention.
    ///
    /// Only produced by `libclang` 3.6 and later.
    Vectorcall = 12,
    /// The function type uses the calling convention for the Swift programming language.
    ///
    /// Only produced by `libclang` 3.9 and later.
    Swift = 13,
    /// The function type uses a calling convention that perserves most registers.
    ///
    /// Only produced by `libclang` 3.9 and later.
    PreserveMost = 14,
    /// The function type uses a calling convention that preverses nearly all registers.
    ///
    /// Only produced by `libclang` 3.9 and later.
    PreserveAll = 15,
    /// The function type uses the ARM AACPS calling convention.
    Aapcs = 6,
    /// The function type uses the ARM AACPS-VFP calling convention.
    AapcsVfp = 7,
    /// The function type uses the calling convention for Intel OpenCL built-ins.
    IntelOcl = 9,
    /// The function type uses the x64 C calling convention as specified in the System V ABI.
    SysV64 = 11,
    /// The function type uses the x64 C calling convention as implemented on Windows.
    Win64 = 10,
}

// EntityKind ____________________________________

/// Indicates the categorization of an AST entity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum EntityKind {
    /// A declaration whose specific type is not exposed via this interface.
    UnexposedDecl = 1,
    /// A C or C++ struct.
    StructDecl = 2,
    /// A C or C++ union.
    UnionDecl = 3,
    /// A C++ class.
    ClassDecl = 4,
    /// An enum.
    EnumDecl = 5,
    /// A C field or C++ non-static data member in a struct, union, or class.
    FieldDecl = 6,
    /// An enum constant.
    EnumConstantDecl = 7,
    /// A function.
    FunctionDecl = 8,
    /// A variable.
    VarDecl = 9,
    /// A parameter.
    ParmDecl = 10,
    /// An Objective-C `@interface`.
    ObjCInterfaceDecl = 11,
    /// An Objective-C `@interface` for a category.
    ObjCCategoryDecl = 12,
    /// An Objective-C `@protocol` declaration.
    ObjCProtocolDecl = 13,
    /// An Objective-C `@property` declaration.
    ObjCPropertyDecl = 14,
    /// An Objective-C instance variable.
    ObjCIvarDecl = 15,
    /// An Objective-C instance method.
    ObjCInstanceMethodDecl = 16,
    /// An Objective-C class method.
    ObjCClassMethodDecl = 17,
    /// An Objective-C `@implementation`.
    ObjCImplementationDecl = 18,
    /// An Objective-C `@implementation` for a category.
    ObjCCategoryImplDecl = 19,
    /// A typedef.
    TypedefDecl = 20,
    /// A C++ method.
    Method = 21,
    /// A C++ namespace.
    Namespace = 22,
    /// A linkage specification (e.g., `extern "C"`).
    LinkageSpec = 23,
    /// A C++ constructor.
    Constructor = 24,
    /// A C++ destructor.
    Destructor = 25,
    /// A C++ conversion function.
    ConversionFunction = 26,
    /// A C++ template type parameter.
    TemplateTypeParameter = 27,
    /// A C++ template non-type parameter.
    NonTypeTemplateParameter = 28,
    /// A C++ template template parameter.
    TemplateTemplateParameter = 29,
    /// A C++ function template.
    FunctionTemplate = 30,
    /// A C++ class template.
    ClassTemplate = 31,
    /// A C++ class template partial specialization.
    ClassTemplatePartialSpecialization = 32,
    /// A C++ namespace alias declaration.
    NamespaceAlias = 33,
    /// A C++ using directive.
    UsingDirective = 34,
    /// A C++ using declaration.
    UsingDeclaration = 35,
    /// A C++ type alias declaration.
    TypeAliasDecl = 36,
    /// An Objective-C `@synthesize` definition.
    ObjCSynthesizeDecl = 37,
    /// An Objective-C `@dynamic` definition.
    ObjCDynamicDecl = 38,
    /// An access specifier.
    AccessSpecifier = 39,
    /// A reference to a super class in Objective-C.
    ObjCSuperClassRef = 40,
    /// A reference to a protocol in Objective-C.
    ObjCProtocolRef = 41,
    /// A reference to a class in Objective-C.
    ObjCClassRef = 42,
    /// A reference to a type declaration.
    TypeRef = 43,
    /// A base class specifier.
    BaseSpecifier = 44,
    /// A reference to a class template, function template, template template parameter, or class
    /// template partial specialization.
    TemplateRef = 45,
    /// A reference to a namespace or namespace alias.
    NamespaceRef = 46,
    /// A reference to a member of a struct, union, or class that occurs in some non-expression
    /// context.
    MemberRef = 47,
    /// A reference to a labeled statement.
    LabelRef = 48,
    /// A reference to a set of overloaded functions or function templates that has not yet been
    /// resolved to a specific function or function template.
    OverloadedDeclRef = 49,
    /// A reference to a variable that occurs in some non-expression context.
    VariableRef = 50,
    /// An expression whose specific kind is not exposed via this interface.
    UnexposedExpr = 100,
    /// An expression that refers to some value declaration, such as a function or enumerator.
    DeclRefExpr = 101,
    /// An expression that refers to the member of a struct, union, or class.
    MemberRefExpr = 102,
    /// An expression that calls a function.
    CallExpr = 103,
    /// An expression that sends a message to an Objective-C object or class.
    ObjCMessageExpr = 104,
    /// An expression that represents a block literal.
    BlockExpr = 105,
    /// An integer literal.
    IntegerLiteral = 106,
    /// A floating point number literal.
    FloatingLiteral = 107,
    /// An imaginary number literal.
    ImaginaryLiteral = 108,
    /// A string literal.
    StringLiteral = 109,
    /// A character literal.
    CharacterLiteral = 110,
    /// A parenthesized expression.
    ParenExpr = 111,
    /// Any unary expression other than `sizeof` and `alignof`.
    UnaryOperator = 112,
    /// An array subscript expression (`[C99 6.5.2.1]`).
    ArraySubscriptExpr = 113,
    /// A built-in binary expression (e.g., `x + y`).
    BinaryOperator = 114,
    /// A compound assignment expression (e.g., `x += y`).
    CompoundAssignOperator = 115,
    /// A ternary expression.
    ConditionalOperator = 116,
    /// An explicit cast in C or a C-style cast in C++.
    CStyleCastExpr = 117,
    /// A compound literal expression (`[C99 6.5.2.5]`).
    CompoundLiteralExpr = 118,
    /// A C or C++ initializer list.
    InitListExpr = 119,
    /// A GNU address of label expression.
    AddrLabelExpr = 120,
    /// A GNU statement expression.
    StmtExpr = 121,
    /// A C11 generic selection expression.
    GenericSelectionExpr = 122,
    /// A GNU `__null` expression.
    GNUNullExpr = 123,
    /// A C++ `static_cast<>` expression.
    StaticCastExpr = 124,
    /// A C++ `dynamic_cast<>` expression.
    DynamicCastExpr = 125,
    /// A C++ `reinterpret_cast<>` expression.
    ReinterpretCastExpr = 126,
    /// A C++ `const_cast<>` expression.
    ConstCastExpr = 127,
    /// A C++ cast that uses "function" notation (e.g., `int(0.5)`).
    FunctionalCastExpr = 128,
    /// A C++ `typeid` expression.
    TypeidExpr = 129,
    /// A C++ boolean literal.
    BoolLiteralExpr = 130,
    /// A C++ `nullptr` expression.
    NullPtrLiteralExpr = 131,
    /// A C++ `this` expression.
    ThisExpr = 132,
    /// A C++ `throw` expression.
    ThrowExpr = 133,
    /// A C++ `new` expression.
    NewExpr = 134,
    /// A C++ `delete` expression.
    DeleteExpr = 135,
    /// A unary expression.
    UnaryExpr = 136,
    /// An Objective-C string literal.
    ObjCStringLiteral = 137,
    /// An Objective-C `@encode` expression.
    ObjCEncodeExpr = 138,
    /// An Objective-C `@selector` expression.
    ObjCSelectorExpr = 139,
    /// An Objective-C `@protocol` expression.
    ObjCProtocolExpr = 140,
    /// An Objective-C bridged cast expression.
    ObjCBridgedCastExpr = 141,
    /// A C++11 parameter pack expansion expression.
    PackExpansionExpr = 142,
    /// A C++11 `sizeof...` expression.
    SizeOfPackExpr = 143,
    /// A C++11 lambda expression.
    LambdaExpr = 144,
    /// An Objective-C boolean literal.
    ObjCBoolLiteralExpr = 145,
    /// An Objective-C `self` expression.
    ObjCSelfExpr = 146,
    /// An OpenMP array section expression.
    ///
    /// Only produced by `libclang` 3.8 and later.
    OmpArraySectionExpr = 147,
    /// An Objective-C availability check expression (e.g., `@available(macos 10.10, *)`).
    ///
    /// Only produced by `libclang` 3.9 and later.
    ObjCAvailabilityCheckExpr = 148,
    /// A statement whose specific kind is not exposed via this interface.
    UnexposedStmt = 200,
    /// A labelled statement in a function.
    LabelStmt = 201,
    /// A group of statements (e.g., a function body).
    CompoundStmt = 202,
    /// A `case` statement.
    CaseStmt = 203,
    /// A `default` statement.
    DefaultStmt = 204,
    /// An `if` statement.
    IfStmt = 205,
    /// A `switch` statement.
    SwitchStmt = 206,
    /// A `while` statement.
    WhileStmt = 207,
    /// A `do` statement.
    DoStmt = 208,
    /// A `for` statement.
    ForStmt = 209,
    /// A `goto` statement.
    GotoStmt = 210,
    /// An indirect `goto` statement.
    IndirectGotoStmt = 211,
    /// A `continue` statement.
    ContinueStmt = 212,
    /// A `break` statement.
    BreakStmt = 213,
    /// A `return` statement.
    ReturnStmt = 214,
    /// An inline assembly statement.
    AsmStmt = 215,
    /// An Objective-C `@try`-`@catch`-`@finally` statement.
    ObjCAtTryStmt = 216,
    /// An Objective-C `@catch` statement.
    ObjCAtCatchStmt = 217,
    /// An Objective-C `@finally` statement.
    ObjCAtFinallyStmt = 218,
    /// An Objective-C `@throw` statement.
    ObjCAtThrowStmt = 219,
    /// An Objective-C `@synchronized` statement.
    ObjCAtSynchronizedStmt = 220,
    /// An Objective-C autorelease pool statement.
    ObjCAutoreleasePoolStmt = 221,
    /// An Objective-C collection statement.
    ObjCForCollectionStmt = 222,
    /// A C++ catch statement.
    CatchStmt = 223,
    /// A C++ try statement.
    TryStmt = 224,
    /// A C++11 range-based for statement.
    ForRangeStmt = 225,
    /// A Windows Structured Exception Handling `__try` statement.
    SehTryStmt = 226,
    /// A Windows Structured Exception Handling `__except` statement.
    SehExceptStmt = 227,
    /// A Windows Structured Exception Handling `__finally` statement.
    SehFinallyStmt = 228,
    /// A Windows Structured Exception Handling `__leave` statement.
    SehLeaveStmt = 247,
    /// A Microsoft inline assembly statement.
    MsAsmStmt = 229,
    /// A null statement.
    NullStmt = 230,
    /// An adaptor for mixing declarations with statements and expressions.
    DeclStmt = 231,
    /// An OpenMP parallel directive.
    OmpParallelDirective = 232,
    /// An OpenMP SIMD directive.
    OmpSimdDirective = 233,
    /// An OpenMP for directive.
    OmpForDirective = 234,
    /// An OpenMP sections directive.
    OmpSectionsDirective = 235,
    /// An OpenMP section directive.
    OmpSectionDirective = 236,
    /// An OpenMP single directive.
    OmpSingleDirective = 237,
    /// An OpenMP parallel for directive.
    OmpParallelForDirective = 238,
    /// An OpenMP parallel sections directive.
    OmpParallelSectionsDirective = 239,
    /// An OpenMP task directive.
    OmpTaskDirective = 240,
    /// An OpenMP master directive.
    OmpMasterDirective = 241,
    /// An OpenMP critical directive.
    OmpCriticalDirective = 242,
    /// An OpenMP taskyield directive.
    OmpTaskyieldDirective = 243,
    /// An OpenMP barrier directive.
    OmpBarrierDirective = 244,
    /// An OpenMP taskwait directive.
    OmpTaskwaitDirective = 245,
    /// An OpenMP flush directive.
    OmpFlushDirective = 246,
    /// An OpenMP ordered directive.
    ///
    /// Only produced by `libclang` 3.6 and later.
    OmpOrderedDirective = 248,
    /// An OpenMP atomic directive.
    ///
    /// Only produced by `libclang` 3.6 and later.
    OmpAtomicDirective = 249,
    /// An OpenMP for SIMD directive.
    ///
    /// Only produced by `libclang` 3.6 and later.
    OmpForSimdDirective = 250,
    /// An OpenMP parallel for SIMD directive.
    ///
    /// Only produced by `libclang` 3.6 and later.
    OmpParallelForSimdDirective = 251,
    /// An OpenMP target directive.
    ///
    /// Only produced by `libclang` 3.6 and later.
    OmpTargetDirective = 252,
    /// An OpenMP teams directive.
    ///
    /// Only produced by `libclang` 3.6 and later.
    OmpTeamsDirective = 253,
    /// An OpenMP taskgroup directive.
    ///
    /// Only produced by `libclang` 3.7 and later.
    OmpTaskgroupDirective = 254,
    /// An OpenMP cancellation point directive.
    ///
    /// Only produced by `libclang` 3.7 and later.
    OmpCancellationPointDirective = 255,
    /// An OpenMP cancel directive.
    ///
    /// Only produced by `libclang` 3.7 and later.
    OmpCancelDirective = 256,
    /// An OpenMP target data directive.
    ///
    /// Only produced by `libclang` 3.8 and later.
    OmpTargetDataDirective = 257,
    /// An OpenMP task loop directive.
    ///
    /// Only produced by `libclang` 3.8 and later.
    OmpTaskLoopDirective = 258,
    /// An OpenMP task loop SIMD directive.
    ///
    /// Only produced by `libclang` 3.8 and later.
    OmpTaskLoopSimdDirective = 259,
    /// An OpenMP distribute directive.
    ///
    /// Only produced by `libclang` 3.8 and later.
    OmpDistributeDirective = 260,
    /// An OpenMP target enter data directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpTargetEnterDataDirective = 261,
    /// An OpenMP target exit data directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpTargetExitDataDirective = 262,
    /// An OpenMP target parallel directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpTargetParallelDirective = 263,
    /// An OpenMP target parallel for directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpTargetParallelForDirective = 264,
    /// An OpenMP target update directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpTargetUpdateDirective = 265,
    /// An OpenMP distribute parallel for directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpDistributeParallelForDirective = 266,
    /// An OpenMP distribute parallel for SIMD directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpDistributeParallelForSimdDirective = 267,
    /// An OpenMP distribute SIMD directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpDistributeSimdDirective = 268,
    /// An OpenMP target parallel for SIMD directive.
    ///
    /// Only produced by `libclang` 3.9 and later.
    OmpTargetParallelForSimdDirective = 269,
    /// The top-level AST entity which acts as the root for the other entitys.
    TranslationUnit = 300,
    /// An attribute whose specific kind is not exposed via this interface.
    UnexposedAttr = 400,
    /// An attribute applied to an Objective-C IBAction.
    IbActionAttr = 401,
    /// An attribute applied to an Objective-C IBOutlet.
    IbOutletAttr = 402,
    /// An attribute applied to an Objective-C IBOutletCollection.
    IbOutletCollectionAttr = 403,
    /// The `final` attribute.
    FinalAttr = 404,
    /// The `override` attribute.
    OverrideAttr = 405,
    /// An annotation attribute.
    AnnotateAttr = 406,
    /// An ASM label attribute.
    AsmLabelAttr = 407,
    /// An attribute that requests for packed records (e.g., `__attribute__ ((__packed__))`).
    PackedAttr = 408,
    /// An attribute that asserts a function has no side effects (e.g., `__attribute__((pure))`).
    PureAttr = 409,
    /// The `const` attribute.
    ConstAttr = 410,
    /// An attribute that allows calls to a function to be duplicated by the optimized
    /// (e.g., `__attribute__((noduplicate))`).
    NoDuplicateAttr = 411,
    /// A CUDA constant attribute.
    CudaConstantAttr = 412,
    /// A CUDA device attribute.
    CudaDeviceAttr = 413,
    /// A CUDA global attribute.
    CudaGlobalAttr = 414,
    /// A CUDA host attribute.
    CudaHostAttr = 415,
    /// A CUDA shared attribute.
    ///
    /// Only produced by `libclang` 3.6 and later.
    CudaSharedAttr = 416,
    /// A linker visibility attribute.
    ///
    /// Only produced by `libclang` 3.8 and later.
    VisibilityAttr = 417,
    /// A MSVC DLL export attribute.
    ///
    /// Only produced by `libclang` 3.8 and later.
    DllExport = 418,
    /// A MSVC DLL import attribute.
    ///
    /// Only produced by `libclang` 3.8 and later.
    DllImport = 419,
    /// A preprocessing directive.
    PreprocessingDirective = 500,
    /// A macro definition.
    MacroDefinition = 501,
    /// A macro expansion.
    MacroExpansion = 502,
    /// An inclusion directive.
    InclusionDirective = 503,
    /// A module import declaration.
    ModuleImportDecl = 600,
    /// A C++11 alias template declaration (e.g., `template <typename T> using M = std::map<T, T>`).
    ///
    /// Only produced by `libclang` 3.8 and later.
    TypeAliasTemplateDecl = 601,
    /// A `static_assert` node.
    ///
    /// Only produced by `libclang` 3.9 and later.
    StaticAssert = 602,
    /// A single overload in a set of overloads.
    ///
    /// Only produced by `libclang` 3.7 and later.
    OverloadCandidate = 700,
}

// EntityVisitResult _____________________________

/// Indicates how a entity visitation should proceed.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum EntityVisitResult {
    /// Do not continue visiting entities.
    Break = 0,
    /// Continue visiting sibling entities iteratively, skipping child entities.
    Continue = 1,
    /// Continue visiting sibling and child entities recursively, children first.
    Recurse = 2,
}

// EvaluationResult ______________________________

/// The result of evaluating an expression.
#[cfg(feature="gte_clang_3_9")]
#[derive(Clone, Debug, PartialEq)]
pub enum EvaluationResult {
    /// An evaluation result whose specific type is not exposed via this interface.
    Unexposed,
    /// An integer evaluation result.
    Integer(i64),
    /// A floating point number evaluation result.
    Float(f64),
    /// A string literal evaluation result.
    String(CString),
    /// An Objective-C string literal evaluation result.
    ObjCString(CString),
    /// An Objective-C `CFString` evaluation result.
    CFString(CString),
    /// Any other evaluation result whose value can be represented by a string.
    Other(CString),
}

// Language ______________________________________

/// Indicates the language used by a declaration.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum Language {
    /// The declaration uses the C programming language.
    C = 1,
    /// The declaration uses the C++ programming language.
    Cpp = 3,
    /// The declaration uses the Objective-C programming language.
    ObjectiveC = 2,
}

// Linkage _______________________________________

/// Indicates the linkage of an AST entity.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum Linkage {
    /// The AST entity has automatic storage (e.g., variables or parameters).
    Automatic = 1,
    /// The AST entity is a static variable or static function.
    Internal = 2,
    /// The AST entity has external linkage.
    External = 4,
    /// The AST entity has external linkage and lives in a C++ anonymous namespace.
    UniqueExternal = 3,
}

// MemoryUsage ___________________________________

/// Indicates the usage category of a quantity of memory.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum MemoryUsage {
    /// Expressions, declarations, and types.
    Ast = 1,
    /// Various tables used by the AST.
    AstSideTables = 6,
    /// Memory allocated with `malloc` for external AST sources.
    ExternalAstSourceMalloc = 9,
    /// Memory allocated with `mmap` for external AST sources.
    ExternalAstSourceMMap = 10,
    /// Cached global code completion results.
    GlobalCodeCompletionResults = 4,
    /// Identifiers.
    Identifiers = 2,
    /// The preprocessing record.
    PreprocessingRecord = 12,
    /// Memory allocated with `malloc` for the preprocessor.
    Preprocessor = 11,
    /// Header search tables.
    PreprocessorHeaderSearch = 14,
    /// Selectors.
    Selectors = 3,
    /// The content cache used by the source manager.
    SourceManagerContentCache = 5,
    /// Data structures used by the source manager.
    SourceManagerDataStructures = 13,
    /// Memory allocated with `malloc` for the source manager.
    SourceManagerMalloc = 7,
    /// Memory allocated with `mmap` for the source manager.
    SourceManagerMMap = 8,
}

// RefQualifier __________________________________

/// Indicates the ref qualifier of a C++ function or method type.
#[cfg_attr(feature="clippy", allow(enum_variant_names))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum RefQualifier {
    /// The function or method has an l-value ref qualifier (`&`).
    LValue = 1,
    /// The function or method has an r-value ref qualifier (`&&`).
    RValue = 2,
}

// StorageClass __________________________________

/// Indicates the storage class of a declaration.
#[cfg(feature="gte_clang_3_6")]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum StorageClass {
    /// The declaration does not specifiy a storage duration and therefore has an automatic storage
    /// duration.
    None = 1,
    /// The declaration specifies an automatic storage duration.
    Auto = 6,
    /// The declaration specifies an automatic storage duration and that it should be stored in a
    /// CPU register
    Register = 7,
    /// The declaration specifies a static storage duration and internal linkage.
    Static = 3,
    /// The declaration specifies a static storage duration and external linkage.
    Extern = 2,
    /// The declaration specifies a static storage duration and external linkage but is not
    /// accessible outside the containing translation unit.
    PrivateExtern = 4,
    /// The declaration specifies a storage duration related to an OpenCL work group.
    OpenClWorkGroupLocal = 5,
}

// TemplateArgument ______________________________

/// An argument to a template function specialization.
#[cfg(feature="gte_clang_3_6")]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TemplateArgument<'tu> {
    /// A declaration for a pointer, reference, or member pointer non-type template parameter.
    Declaration,
    /// An expression that has not yet been resolved
    Expression,
    /// An empty template argument (e.g., one that has not yet been deduced).
    Null,
    /// A null pointer or null member pointer provided for a non-type template parameter.
    Nullptr,
    /// A parameter pack.
    Pack,
    /// A name for a template provided for a template template parameter.
    Template,
    /// A pack expansion of a name for a template provided for a template template parameter.
    TemplateExpansion,
    /// An integer.
    Integral(i64, u64),
    /// A type.
    Type(Type<'tu>),
}

// TypeKind ______________________________________

/// Indicates the categorization of a type.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum TypeKind {
    /// A type whose specific kind is not exposed via this interface.
    Unexposed = 1,
    /// `void`
    Void = 2,
    /// `bool` (C++) or `_Bool` (C99)
    Bool = 3,
    /// The `char` type when it is signed by default.
    CharS = 13,
    /// The `char` type when it is unsigned by default.
    CharU = 4,
    /// `signed char`
    SChar = 14,
    /// `unsigned char`
    UChar = 5,
    /// `wchar_t`
    WChar = 15,
    /// `char16_t`
    Char16 = 6,
    /// `char32_t`
    Char32 = 7,
    /// `short`
    Short = 16,
    /// `unsigned short`
    UShort = 8,
    /// `int`
    Int = 17,
    /// `unsigned int`
    UInt = 9,
    /// `long`
    Long = 18,
    /// `unsigned long`
    ULong = 10,
    /// `long long`
    LongLong = 19,
    /// `unsigned long long`
    ULongLong = 11,
    /// `__int128_t`
    Int128 = 20,
    /// `__uint128_t`
    UInt128 = 12,
    /// `float`
    Float = 21,
    /// `double`
    Double = 22,
    /// `long double`
    LongDouble = 23,
    /// `nullptr_t` (C++11)
    Nullptr = 24,
    /// A C99 complex type (e.g., `_Complex float`).
    Complex = 100,
    /// An unknown dependent type.
    Dependent = 26,
    /// The type of an unresolved overload set.
    Overload = 25,
    /// `id` (Objective-C)
    ObjCId = 27,
    /// `Class` (Objective-C)
    ObjCClass = 28,
    /// `SEL` (Objective-C)
    ObjCSel = 29,
    /// `__float128`
    ///
    /// Only produced by `libclang` 3.9 and later.
    Float128 = 30,
    /// An Objective-C interface type.
    ObjCInterface = 108,
    /// An Objective-C pointer to object type.
    ObjCObjectPointer = 109,
    /// A pointer type.
    Pointer = 101,
    /// A block pointer type (e.g., `void (^)(int)`).
    BlockPointer = 102,
    /// A pointer to a record member type.
    MemberPointer = 117,
    /// An l-value reference (e.g. `int&`).
    LValueReference = 103,
    /// An r-value reference (e.g. `int&&`).
    RValueReference = 104,
    /// An enum type.
    Enum = 106,
    /// A record type such as a struct or a class.
    Record = 105,
    /// A typedef.
    Typedef = 107,
    /// A function prototype with parameter type information (e.g., `void foo(int)`).
    FunctionPrototype = 111,
    /// A function prototype without parameter type information (e.g., `void foo()`).
    FunctionNoPrototype = 110,
    /// An array type with a specified size that is an integer constant expression.
    ConstantArray = 112,
    /// An array type with a specified size that is a dependent value.
    DependentSizedArray = 116,
    /// An array type without a specified size.
    IncompleteArray = 114,
    /// An array type with a specified size that is not an integer constant expression.
    VariableArray = 115,
    /// A GCC generic vector type.
    Vector = 113,
    /// A C++11 `decltype(auto)` type.
    ///
    /// Only produced by `libclang` 3.8 and later.
    Auto = 118,
    /// A type that was referred to using an elaborated type keyword (e.g., `struct S`).
    ///
    /// Only produced by `libclang` 3.9 and later.
    Elaborated = 119,
}

// Visibility ____________________________________

/// Indicates the linker visibility of an AST element.
#[cfg(feature="gte_clang_3_8")]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum Visibility {
    /// The AST element can be seen by the linker.
    Default = 3,
    /// The AST element cannot be seen by the linker.
    Hidden = 1,
    /// The AST element can be seen by the linker but resolves to a symbol inside this object.
    Protected = 2,
}

//================================================
// Structs
//================================================

// Clang _________________________________________

lazy_static! { static ref AVAILABLE: AtomicBool = AtomicBool::new(true); }

/// An empty type which prevents the use of this library from multiple threads simultaneously.
#[derive(Debug)]
pub struct Clang;

impl Clang {
    //- Constructors -----------------------------

    /// Constructs a new `Clang`.
    ///
    /// Only one instance of `Clang` is allowed at a time.
    ///
    /// # Failures
    ///
    /// * an instance of `Clang` already exists
    /// * a `libclang` shared library could not be found
    /// * a `libclang` shared library symbol could not be loaded
    #[cfg(feature="runtime")]
    pub fn new() -> Result<Clang, String> {
        if AVAILABLE.swap(false, atomic::Ordering::SeqCst) {
            load().map(|_| Clang)
        } else {
            Err("an instance of `Clang` already exists".into())
        }
    }

    /// Constructs a new `Clang`.
    ///
    /// Only one instance of `Clang` is allowed at a time.
    ///
    /// # Failures
    ///
    /// * an instance of `Clang` already exists
    #[cfg(not(feature="runtime"))]
    pub fn new() -> Result<Clang, String> {
        if AVAILABLE.swap(false, atomic::Ordering::SeqCst) {
            Ok(Clang)
        } else {
            Err("an instance of `Clang` already exists".into())
        }
    }
}

#[cfg(feature="runtime")]
impl Drop for Clang {
    fn drop(&mut self) {
        unload().unwrap();
        AVAILABLE.store(true, atomic::Ordering::SeqCst);
    }
}

#[cfg(not(feature="runtime"))]
impl Drop for Clang {
    fn drop(&mut self) {
        AVAILABLE.store(true, atomic::Ordering::SeqCst);
    }
}

// Entity ________________________________________

/// An AST entity.
#[derive(Copy, Clone)]
pub struct Entity<'tu> {
    raw: CXCursor,
    tu: &'tu TranslationUnit<'tu>,
}

impl<'tu> Entity<'tu> {
    //- Constructors -----------------------------

    fn from_raw(raw: CXCursor, tu: &'tu TranslationUnit<'tu>) -> Entity<'tu> {
        Entity { raw: raw, tu: tu }
    }

    //- Accessors --------------------------------

    /// Evaluates this AST entity, if possible.
    #[cfg(feature="gte_clang_3_9")]
    pub fn evaluate(&self) -> Option<EvaluationResult> {
        macro_rules! string {
            ($eval:expr) => {
                std::ffi::CStr::from_ptr(clang_EvalResult_getAsStr($eval)).to_owned()
            };
        }

        unsafe {
            clang_Cursor_Evaluate(self.raw).map(|e| {
                let result = match clang_EvalResult_getKind(e) {
                    CXEval_UnExposed => EvaluationResult::Unexposed,
                    CXEval_Int => EvaluationResult::Integer(clang_EvalResult_getAsInt(e) as i64),
                    CXEval_Float => EvaluationResult::Float(clang_EvalResult_getAsDouble(e) as f64),
                    CXEval_ObjCStrLiteral => EvaluationResult::ObjCString(string!(e)),
                    CXEval_StrLiteral => EvaluationResult::String(string!(e)),
                    CXEval_CFStr => EvaluationResult::CFString(string!(e)),
                    CXEval_Other => EvaluationResult::Other(string!(e)),
                    _ => panic!("unexpected eval result: {:?}", e),
                };
                clang_EvalResult_dispose(e);
                result
            })
        }
    }

    /// Returns the categorization of this AST entity.
    pub fn get_kind(&self) -> EntityKind {
        unsafe { mem::transmute(clang_getCursorKind(self.raw)) }
    }

    /// Returns the display name of this AST entity, if any.
    ///
    /// The display name of an entity contains additional information that helps identify the
    /// entity.
    pub fn get_display_name(&self) -> Option<String> {
        unsafe { utility::to_string_option(clang_getCursorDisplayName(self.raw)) }
    }

    /// Returns the source location of this AST entity, if any.
    pub fn get_location(&self) -> Option<SourceLocation<'tu>> {
        unsafe { clang_getCursorLocation(self.raw).map(|l| SourceLocation::from_raw(l, self.tu)) }
    }

    /// Returns the source range of this AST entity, if any.
    pub fn get_range(&self) -> Option<SourceRange<'tu>> {
        unsafe { clang_getCursorExtent(self.raw).map(|r| SourceRange::from_raw(r, self.tu)) }
    }

    /// Returns the accessibility of this declaration or base class specifier, if applicable.
    pub fn get_accessibility(&self) -> Option<Accessibility> {
        unsafe {
            match clang_getCXXAccessSpecifier(self.raw) {
                CX_CXXInvalidAccessSpecifier => None,
                other => Some(mem::transmute(other)),
            }
        }
    }

    /// Returns the arguments of this function or method, if applicable.
    pub fn get_arguments(&self) -> Option<Vec<Entity<'tu>>> {
        iter_option!(
            clang_Cursor_getNumArguments(self.raw),
            clang_Cursor_getArgument(self.raw),
        ).map(|i| i.map(|a| Entity::from_raw(a, self.tu)).collect())
    }

    /// Returns the availability of this AST entity.
    pub fn get_availability(&self) -> Availability {
        unsafe { mem::transmute(clang_getCursorAvailability(self.raw)) }
    }

    /// Returns the width of this bit field, if applicable.
    pub fn get_bit_field_width(&self) -> Option<usize> {
        unsafe {
            let width = clang_getFieldDeclBitWidth(self.raw);
            if width >= 0 {
                Some(width as usize)
            } else {
                None
            }
        }
    }

    /// Returns the canonical entity for this AST entity.
    ///
    /// In the C family of languages, some types of entities can be declared multiple times. When
    /// there are multiple declarations of the same entity, only one will be considered canonical.
    pub fn get_canonical_entity(&self) -> Entity<'tu> {
        unsafe { Entity::from_raw(clang_getCanonicalCursor(self.raw), self.tu) }
    }

    /// Returns the comment associated with this AST entity, if any.
    pub fn get_comment(&self) -> Option<String> {
        unsafe { utility::to_string_option(clang_Cursor_getRawCommentText(self.raw)) }
    }

    ///  Returns the parsed comment associated with this declaration, if application.
    pub fn get_parsed_comment(&self) -> Option<Comment<'tu>> {
        unsafe { clang_Cursor_getParsedComment(self.raw).map(Comment::from_raw) }
    }

    /// Returns the brief of the comment associated with this AST entity, if any.
    pub fn get_comment_brief(&self) -> Option<String> {
        unsafe { utility::to_string_option(clang_Cursor_getBriefCommentText(self.raw)) }
    }

    /// Returns the source range of the comment associated with this AST entity, if any.
    pub fn get_comment_range(&self) -> Option<SourceRange<'tu>> {
        unsafe { clang_Cursor_getCommentRange(self.raw).map(|r| SourceRange::from_raw(r, self.tu)) }
    }

    /// Returns a completion string for this declaration or macro definition, if applicable.
    pub fn get_completion_string(&self) -> Option<CompletionString> {
        unsafe { clang_getCursorCompletionString(self.raw).map(CompletionString::from_ptr) }
    }

    /// Returns the child of this AST entity with the supplied index.
    pub fn get_child(&self, mut index: usize) -> Option<Entity<'tu>> {
        let mut child = None;
        self.visit_children(|c, _| {
            if index == 0 {
                child = Some(c);
                EntityVisitResult::Break
            } else {
                index -= 1;
                EntityVisitResult::Continue
            }
        });
        child
    }

    /// Returns the children of this AST entity.
    pub fn get_children(&self) -> Vec<Entity<'tu>> {
        let mut children = vec![];
        self.visit_children(|c, _| {
            children.push(c);
            EntityVisitResult::Continue
        });
        children
    }

    /// Returns the AST entity that describes the definition of this AST entity, if any.
    pub fn get_definition(&self) -> Option<Entity<'tu>> {
        unsafe { clang_getCursorDefinition(self.raw).map(|p| Entity::from_raw(p, self.tu)) }
    }

    /// Returns the value of this enum constant declaration, if applicable.
    pub fn get_enum_constant_value(&self) -> Option<(i64, u64)> {
        unsafe {
            if self.get_kind() == EntityKind::EnumConstantDecl {
                let signed = clang_getEnumConstantDeclValue(self.raw);
                let unsigned = clang_getEnumConstantDeclUnsignedValue(self.raw);
                Some((signed, unsigned))
            } else {
                None
            }
        }
    }

    /// Returns the underlying type of this enum declaration, if applicable.
    pub fn get_enum_underlying_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_getEnumDeclIntegerType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the file included by this inclusion directive, if applicable.
    pub fn get_file(&self) -> Option<File<'tu>> {
        unsafe { clang_getIncludedFile(self.raw).map(|f| File::from_ptr(f, self.tu)) }
    }

    /// Returns the language used by this declaration, if applicable.
    pub fn get_language(&self) -> Option<Language> {
        unsafe {
            match clang_getCursorLanguage(self.raw) {
                CXLanguage_Invalid => None,
                other => Some(mem::transmute(other)),
            }
        }
    }

    /// Returns the lexical parent of this AST entity, if any.
    pub fn get_lexical_parent(&self) -> Option<Entity<'tu>> {
        unsafe { clang_getCursorLexicalParent(self.raw).map(|p| Entity::from_raw(p, self.tu)) }
    }

    /// Returns the linkage of this AST entity, if any.
    pub fn get_linkage(&self) -> Option<Linkage> {
        unsafe {
            match clang_getCursorLinkage(self.raw) {
                CXLinkage_Invalid => None,
                other => Some(mem::transmute(other)),
            }
        }
    }

    /// Returns the mangled name of this AST entity, if any.
    #[cfg(feature="gte_clang_3_6")]
    pub fn get_mangled_name(&self) -> Option<String> {
        unsafe { utility::to_string_option(clang_Cursor_getMangling(self.raw)) }
    }

    /// Returns the mangled names of this C++ constructor or destructor, if applicable.
    #[cfg(feature="gte_clang_3_8")]
    pub fn get_mangled_names(&self) -> Option<Vec<String>> {
        unsafe { utility::to_string_set_option(clang_Cursor_getCXXManglings(self.raw)) }
    }

    /// Returns the module imported by this module import declaration, if applicable.
    pub fn get_module(&self) -> Option<Module<'tu>> {
        unsafe { clang_Cursor_getModule(self.raw).map(|m| Module::from_ptr(m, self.tu)) }
    }

    /// Returns the name of this AST entity, if any.
    pub fn get_name(&self) -> Option<String> {
        unsafe { utility::to_string_option(clang_getCursorSpelling(self.raw)) }
    }

    /// Returns the source ranges of the name of this AST entity.
    pub fn get_name_ranges(&self) -> Vec<SourceRange<'tu>> {
        unsafe {
            (0..).map(|i| clang_Cursor_getSpellingNameRange(self.raw, i, 0)).take_while(|r| {
                if clang_Range_isNull(*r) != 0 {
                    false
                } else {
                    let range = clang_getRangeStart(*r);
                    let mut file = ptr::null_mut();
                    let null = ptr::null_mut();
                    clang_getSpellingLocation(range, &mut file, null, null, null);
                    !file.is_null()
                }
            }).map(|r| SourceRange::from_raw(r, self.tu)).collect()
        }
    }

    /// Returns which attributes were applied to this Objective-C property, if applicable.
    pub fn get_objc_attributes(&self) -> Option<ObjCAttributes> {
        let attributes = unsafe { clang_Cursor_getObjCPropertyAttributes(self.raw, 0) };
        if attributes.bits() != 0 {
            Some(ObjCAttributes::from(attributes))
        } else {
            None
        }
    }

    /// Returns the element type for this Objective-C `iboutletcollection` attribute, if applicable.
    pub fn get_objc_ib_outlet_collection_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_getIBOutletCollectionType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the type of the receiver of this Objective-C message, if applicable.
    pub fn get_objc_receiver_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_Cursor_getReceiverType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the selector index for this Objective-C selector identifier, if applicable.
    pub fn get_objc_selector_index(&self) -> Option<usize> {
        let index = unsafe { clang_Cursor_getObjCSelectorIndex(self.raw) };
        if index >= 0 {
            Some(index as usize)
        } else {
            None
        }
    }

    /// Returns the type encoding for this Objective-C declaration, if applicable.
    pub fn get_objc_type_encoding(&self) -> Option<String> {
        unsafe { utility::to_string_option(clang_getDeclObjCTypeEncoding(self.raw)) }
    }

    /// Returns which qualifiers were applied to this Objective-C method return or parameter type,
    /// if applicable.
    pub fn get_objc_qualifiers(&self) -> Option<ObjCQualifiers> {
        let qualifiers = unsafe { clang_Cursor_getObjCDeclQualifiers(self.raw) };
        if qualifiers.bits() != 0 {
            Some(ObjCQualifiers::from(qualifiers))
        } else {
            None
        }
    }

    /// Returns the overloaded declarations referenced by this overloaded declaration reference, if
    /// applicable.
    pub fn get_overloaded_declarations(&self) -> Option<Vec<Entity<'tu>>> {
        let declarations = iter!(
            clang_getNumOverloadedDecls(self.raw),
            clang_getOverloadedDecl(self.raw),
        ).map(|e| Entity::from_raw(e, self.tu)).collect::<Vec<_>>();
        if !declarations.is_empty() {
            Some(declarations)
        } else {
            None
        }
    }

    /// Returns the methods that were overridden by this method, if applicable.
    pub fn get_overridden_methods(&self) -> Option<Vec<Entity<'tu>>> {
        unsafe {
            let (mut raw, mut count) = (ptr::null_mut(), 0);
            clang_getOverriddenCursors(self.raw, &mut raw, &mut count);
            if !raw.is_null() {
                let raws = slice::from_raw_parts(raw, count as usize);
                let methods = raws.iter().map(|e| Entity::from_raw(*e, self.tu)).collect();
                clang_disposeOverriddenCursors(raw);
                Some(methods)
            } else {
                None
            }
        }
    }

    /// Returns the availability of this declaration on the platforms where it is known, if
    /// applicable.
    pub fn get_platform_availability(&self) -> Option<Vec<PlatformAvailability>> {
        if !self.is_declaration() {
            return None;
        }

        unsafe {
            let mut buffer: [CXPlatformAvailability; 32] = [CXPlatformAvailability::default(); 32];
            let count = clang_getCursorPlatformAvailability(
                self.raw,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                (&mut buffer).as_mut_ptr(),
                buffer.len() as c_int,
            );
            Some((0..count as usize).map(|i| PlatformAvailability::from_raw(buffer[i])).collect())
        }
    }

    /// Returns the AST entity referred to by this AST entity, if any.
    pub fn get_reference(&self) -> Option<Entity<'tu>> {
        unsafe { clang_getCursorReferenced(self.raw).map(|p| Entity::from_raw(p, self.tu)) }
    }

    /// Returns the semantic parent of this AST entity, if any.
    pub fn get_semantic_parent(&self) -> Option<Entity<'tu>> {
        let parent = unsafe { clang_getCursorSemanticParent(self.raw) };
        parent.map(|p| Entity::from_raw(p, self.tu))
    }

    /// Returns the storage class of this declaration, if applicable.
    #[cfg(feature="gte_clang_3_6")]
    pub fn get_storage_class(&self) -> Option<StorageClass> {
        unsafe {
            match clang_Cursor_getStorageClass(self.raw) {
                CX_SC_Invalid => None,
                other => Some(mem::transmute(other)),
            }
        }
    }

    /// Returns the template declaration this template specialization was instantiated from, if
    /// applicable.
    pub fn get_template(&self) -> Option<Entity<'tu>> {
        let parent = unsafe { clang_getSpecializedCursorTemplate(self.raw) };
        parent.map(|p| Entity::from_raw(p, self.tu))
    }

    /// Returns the template arguments for this template function specialization, if applicable.
    #[cfg(feature="gte_clang_3_6")]
    pub fn get_template_arguments(&self) -> Option<Vec<TemplateArgument<'tu>>> {
        let get_type = &clang_Cursor_getTemplateArgumentType;
        let get_signed = &clang_Cursor_getTemplateArgumentValue;
        let get_unsigned = &clang_Cursor_getTemplateArgumentUnsignedValue;

        iter_option!(
            clang_Cursor_getNumTemplateArguments(self.raw),
            clang_Cursor_getTemplateArgumentKind(self.raw),
        ).map(|i| {
            i.enumerate().map(|(i, t)| {
                match t {
                    CXTemplateArgumentKind_Null => TemplateArgument::Null,
                    CXTemplateArgumentKind_Type => {
                        let type_ = unsafe { get_type(self.raw, i as c_uint) };
                        TemplateArgument::Type(Type::from_raw(type_, self.tu))
                    },
                    CXTemplateArgumentKind_Declaration => TemplateArgument::Declaration,
                    CXTemplateArgumentKind_NullPtr => TemplateArgument::Nullptr,
                    CXTemplateArgumentKind_Integral => {
                        let signed = unsafe { get_signed(self.raw, i as c_uint) };
                        let unsigned = unsafe { get_unsigned(self.raw, i as c_uint) };
                        TemplateArgument::Integral(signed as i64, unsigned as u64)
                    },
                    CXTemplateArgumentKind_Template => TemplateArgument::Template,
                    CXTemplateArgumentKind_TemplateExpansion => TemplateArgument::TemplateExpansion,
                    CXTemplateArgumentKind_Expression => TemplateArgument::Expression,
                    CXTemplateArgumentKind_Pack => TemplateArgument::Pack,
                    _ => unreachable!(),
                }
            }).collect()
        })
    }

    /// Returns the categorization of the template specialization that would result from
    /// instantiating this template declaration, if applicable.
    pub fn get_template_kind(&self) -> Option<EntityKind> {
        unsafe {
            match clang_getTemplateCursorKind(self.raw) {
                CXCursor_NoDeclFound => None,
                other => Some(mem::transmute(other)),
            }
        }
    }

    /// Returns the translation unit which contains this AST entity.
    pub fn get_translation_unit(&self) -> &'tu TranslationUnit<'tu> {
        self.tu
    }

    /// Returns the type of this AST entity, if any.
    pub fn get_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_getCursorType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the underlying type of this typedef declaration, if applicable.
    pub fn get_typedef_underlying_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_getTypedefDeclUnderlyingType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the USR for this AST entity, if any.
    pub fn get_usr(&self) -> Option<Usr> {
        unsafe { utility::to_string_option(clang_getCursorUSR(self.raw)).map(Usr) }
    }

    /// Returns the linker visibility for this AST entity, if any.
    #[cfg(feature="gte_clang_3_8")]
    pub fn get_visibility(&self) -> Option<Visibility> {
        unsafe {
            match clang_getCursorVisibility(self.raw) {
                CXVisibility_Invalid => None,
                other => Some(mem::transmute(other)),
            }
        }
    }

    /// Returns whether this AST entity has any attached attributes.
    #[cfg(feature="gte_clang_3_9")]
    pub fn has_attributes(&self) -> bool {
        unsafe { clang_Cursor_hasAttrs(self.raw) != 0 }
    }

    /// Returns whether this AST entity is an anonymous record declaration.
    #[cfg(feature="gte_clang_3_7")]
    pub fn is_anonymous(&self) -> bool {
        unsafe { clang_Cursor_isAnonymous(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a bit field.
    pub fn is_bit_field(&self) -> bool {
        unsafe { clang_Cursor_isBitField(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a builtin macro.
    #[cfg(feature="gte_clang_3_9")]
    pub fn is_builtin_macro(&self) -> bool {
        unsafe { clang_Cursor_isMacroBuiltin(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a const method.
    pub fn is_const_method(&self) -> bool {
        unsafe { clang_CXXMethod_isConst(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a C++ converting constructor.
    #[cfg(feature="gte_clang_3_9")]
    pub fn is_converting_constructor(&self) -> bool {
        unsafe { clang_CXXConstructor_isConvertingConstructor(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a C++ copy constructor.
    #[cfg(feature="gte_clang_3_9")]
    pub fn is_copy_constructor(&self) -> bool {
        unsafe { clang_CXXConstructor_isCopyConstructor(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a C++ default constructor.
    #[cfg(feature="gte_clang_3_9")]
    pub fn is_default_constructor(&self) -> bool {
        unsafe { clang_CXXConstructor_isDefaultConstructor(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a C++ defaulted constructor or method.
    #[cfg(feature="gte_clang_3_9")]
    pub fn is_defaulted(&self) -> bool {
        unsafe { clang_CXXMethod_isDefaulted(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a declaration and also the definition of that
    /// declaration.
    pub fn is_definition(&self) -> bool {
        unsafe { clang_isCursorDefinition(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a dynamic call.
    ///
    /// A dynamic call is either a call to a C++ virtual method or an Objective-C message where the
    /// receiver is an object instance, not `super` or a specific class.
    pub fn is_dynamic_call(&self) -> bool {
        unsafe { clang_Cursor_isDynamicCall(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a function-like macro.
    #[cfg(feature="gte_clang_3_9")]
    pub fn is_function_like_macro(&self) -> bool {
        unsafe { clang_Cursor_isFunctionInlined(self.raw) != 0 }
    }

    /// Returns whether this AST entity is an inline function.
    #[cfg(feature="gte_clang_3_9")]
    pub fn is_inline_function(&self) -> bool {
        unsafe { clang_Cursor_isFunctionInlined(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a C++ default constructor.
    #[cfg(feature="gte_clang_3_9")]
    pub fn is_move_constructor(&self) -> bool {
        unsafe { clang_CXXConstructor_isMoveConstructor(self.raw) != 0 }
    }

    #[cfg(feature="gte_clang_3_8")]
    /// Returns whether this AST entity is a mutable field in a C++ struct or class.
    pub fn is_mutable(&self) -> bool {
        unsafe { clang_CXXField_isMutable(self.raw) != 0 }
    }

    /// Returns whether this AST entity is an Objective-C method or property declaration with the
    /// `@optional` attribute applied to it.
    pub fn is_objc_optional(&self) -> bool {
        unsafe { clang_Cursor_isObjCOptional(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a pure virtual method.
    pub fn is_pure_virtual_method(&self) -> bool {
        unsafe { clang_CXXMethod_isPureVirtual(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a static method.
    pub fn is_static_method(&self) -> bool {
        unsafe { clang_CXXMethod_isStatic(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a variadic function or method.
    pub fn is_variadic(&self) -> bool {
        unsafe { clang_Cursor_isVariadic(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a virtual base class specifier.
    pub fn is_virtual_base(&self) -> bool {
        unsafe { clang_isVirtualBase(self.raw) != 0 }
    }

    /// Returns whether this AST entity is a virtual method.
    pub fn is_virtual_method(&self) -> bool {
        unsafe { clang_CXXMethod_isVirtual(self.raw) != 0 }
    }

    /// Visits the children of this AST entity recursively and returns whether visitation was ended
    /// by the callback returning `EntityVisitResult::Break`.
    ///
    /// The first argument of the callback is the AST entity being visited and the second argument
    /// is the parent of that AST entity. The return value of the callback determines how visitation
    /// will proceed.
    pub fn visit_children<F: FnMut(Entity<'tu>, Entity<'tu>) -> EntityVisitResult>(
        &self, f: F
    ) -> bool {
        trait EntityCallback<'tu> {
            fn call(&mut self, entity: Entity<'tu>, parent: Entity<'tu>) -> EntityVisitResult;
        }

        impl<'tu, F: FnMut(Entity<'tu>, Entity<'tu>) -> EntityVisitResult>
        EntityCallback<'tu> for F {
            fn call(&mut self, entity: Entity<'tu>, parent: Entity<'tu>) -> EntityVisitResult {
                self(entity, parent)
            }
        }

        extern fn visit(
            cursor: CXCursor, parent: CXCursor, data: CXClientData
        ) -> CXChildVisitResult {
            unsafe {
                let &mut (tu, ref mut callback) =
                    &mut *(data as *mut (&TranslationUnit, Box<EntityCallback>));

                let entity = Entity::from_raw(cursor, tu);
                let parent = Entity::from_raw(parent, tu);
                mem::transmute(callback.call(entity, parent))
            }
        }

        let mut data = (self.tu, Box::new(f) as Box<EntityCallback>);
        unsafe { clang_visitChildren(self.raw, visit, utility::addressof(&mut data)) != 0 }
    }

    //- Categorization ---------------------------

    /// Returns whether this AST entity is categorized as an attribute.
    pub fn is_attribute(&self) -> bool {
        unsafe { clang_isAttribute(self.raw.kind) != 0 }
    }

    /// Returns whether this AST entity is categorized as a declaration.
    pub fn is_declaration(&self) -> bool {
        unsafe { clang_isDeclaration(self.raw.kind) != 0 }
    }

    /// Returns whether this AST entity is categorized as an expression.
    pub fn is_expression(&self) -> bool {
        unsafe { clang_isExpression(self.raw.kind) != 0 }
    }

    /// Returns whether this AST entity is categorized as a preprocessing entity.
    pub fn is_preprocessing(&self) -> bool {
        unsafe { clang_isPreprocessing(self.raw.kind) != 0 }
    }

    /// Returns whether this AST entity is categorized as a reference.
    pub fn is_reference(&self) -> bool {
        unsafe { clang_isReference(self.raw.kind) != 0 }
    }

    /// Returns whether this AST entity is categorized as a statement.
    pub fn is_statement(&self) -> bool {
        unsafe { clang_isStatement(self.raw.kind) != 0 }
    }

    /// Returns whether the categorization of this AST entity is unexposed.
    pub fn is_unexposed(&self) -> bool {
        unsafe { clang_isUnexposed(self.raw.kind) != 0 }
    }

    //- Location ---------------------------------

    /// Returns whether this AST entity is in a main file.
    pub fn is_in_main_file(&self) -> bool {
        self.get_range().map_or(false, |r| r.is_in_main_file())
    }

    /// Returns whether this AST entity is in a system header.
    pub fn is_in_system_header(&self) -> bool {
        self.get_range().map_or(false, |r| r.is_in_system_header())
    }
}

impl<'tu> fmt::Debug for Entity<'tu> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.debug_struct("Entity")
            .field("kind", &self.get_kind())
            .field("display_name", &self.get_display_name())
            .field("location", &self.get_location())
            .finish()
    }
}

impl<'tu> cmp::PartialEq for Entity<'tu> {
    fn eq(&self, other: &Entity<'tu>) -> bool {
        unsafe { clang_equalCursors(self.raw, other.raw) != 0 }
    }
}

impl<'tu> cmp::Eq for Entity<'tu> { }

impl<'tu> hash::Hash for Entity<'tu> {
    fn hash<H: hash::Hasher>(&self, hasher: &mut H) {
        unsafe {
            let integer = clang_hashCursor(self.raw);
            let bytes = (&integer as *const c_uint) as *const u8;
            let slice = slice::from_raw_parts(bytes, mem::size_of_val(&integer));
            hasher.write(slice);
        }
    }
}

// Index _________________________________________

/// A collection of translation units.
pub struct Index<'c> {
    ptr: CXIndex,
    _marker: PhantomData<&'c Clang>,
}

impl<'c> Index<'c> {
    //- Constructors -----------------------------

    fn from_ptr(ptr: CXIndex) -> Index<'c> {
        assert!(!ptr.is_null());
        Index { ptr: ptr, _marker: PhantomData }
    }

    /// Constructs a new `Index`.
    ///
    /// `exclude` determines whether declarations from precompiled headers are excluded and
    /// `diagnostics` determines whether diagnostics are printed while parsing source files.
    pub fn new(_: &'c Clang, exclude: bool, diagnostics: bool) -> Index<'c> {
        unsafe { Index::from_ptr(clang_createIndex(exclude as c_int, diagnostics as c_int)) }
    }

    //- Accessors --------------------------------

    /// Returns a parser for the supplied file.
    pub fn parser<F: Into<PathBuf>>(&'c self, f: F) -> Parser<'c> {
        Parser::new(self, f)
    }

    /// Returns the thread options for this index.
    pub fn get_thread_options(&self) -> ThreadOptions {
        unsafe { ThreadOptions::from(clang_CXIndex_getGlobalOptions(self.ptr)) }
    }

    //- Mutators ---------------------------------

    /// Sets the thread options for this index.
    pub fn set_thread_options(&mut self, options: ThreadOptions) {
        unsafe { clang_CXIndex_setGlobalOptions(self.ptr, options.into()); }
    }
}

impl<'c> Drop for Index<'c> {
    fn drop(&mut self) {
        unsafe { clang_disposeIndex(self.ptr); }
    }
}

impl<'c> fmt::Debug for Index<'c> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.debug_struct("Index")
            .field("thread_options", &self.get_thread_options())
            .finish()
    }
}

// ObjCAttributes ________________________________

options! {
    /// Indicates which attributes were applied to an Objective-C property.
    options ObjCAttributes: CXObjCPropertyAttrKind {
        /// Indicates use of the `readonly` attribute.
        pub readonly: CXObjCPropertyAttr_readonly,
        /// Indicates use of the `getter` attribute.
        pub getter: CXObjCPropertyAttr_getter,
        /// Indicates use of the `assign` attribute.
        pub assign: CXObjCPropertyAttr_assign,
        /// Indicates use of the `readwrite` attribute.
        pub readwrite: CXObjCPropertyAttr_readwrite,
        /// Indicates use of the `retain` attribute.
        pub retain: CXObjCPropertyAttr_retain,
        /// Indicates use of the `copy` attribute.
        pub copy: CXObjCPropertyAttr_copy,
        /// Indicates use of the `nonatomic` attribute.
        pub nonatomic: CXObjCPropertyAttr_nonatomic,
        /// Indicates use of the `setter` attribute.
        pub setter: CXObjCPropertyAttr_setter,
        /// Indicates use of the `atomic` attribute.
        pub atomic: CXObjCPropertyAttr_atomic,
        /// Indicates use of the `weak` attribute.
        pub weak: CXObjCPropertyAttr_weak,
        /// Indicates use of the `strong` attribute.
        pub strong: CXObjCPropertyAttr_strong,
        /// Indicates use of the `unsafe_retained` attribute.
        pub unsafe_retained: CXObjCPropertyAttr_unsafe_unretained,
    }, objcattributes: #[feature="gte_clang_3_9"] {
        /// Indicates use of the `class` attribute.
        pub class: CXObjCPropertyAttr_class,
    }
}

// ObjCQualifiers ________________________________

options! {
    /// Indicates which qualifiers were applied to an Objective-C method return or parameter type.
    options ObjCQualifiers: CXObjCDeclQualifierKind {
        /// Indicates use of the `in` qualifier.
        pub in_: CXObjCDeclQualifier_In,
        /// Indicates use of the `inout` qualifier.
        pub inout: CXObjCDeclQualifier_Inout,
        /// Indicates use of the `out` qualifier.
        pub out: CXObjCDeclQualifier_Out,
        /// Indicates use of the `bycopy` qualifier.
        pub bycopy: CXObjCDeclQualifier_Bycopy,
        /// Indicates use of the `byref` qualifier.
        pub byref: CXObjCDeclQualifier_Byref,
        /// Indicates use of the `oneway` qualifier.
        pub oneway: CXObjCDeclQualifier_Oneway,
    }
}

// Parser ________________________________________

builder! {
    /// Parses translation units.
    builder Parser: CXTranslationUnit_Flags {
        index: &'tu Index<'tu>,
        file: PathBuf,
        arguments: Vec<CString>,
        unsaved: Vec<Unsaved>;
    OPTIONS:
        /// Sets whether certain code completion results will be cached when the translation unit is
        /// reparsed.
        ///
        /// This option increases the time it takes to reparse the translation unit but improves
        /// code completion performance.
        pub cache_completion_results: CXTranslationUnit_CacheCompletionResults,
        /// Sets whether a detailed preprocessing record will be constructed which tracks all macro
        /// definitions and instantiations.
        pub detailed_preprocessing_record: CXTranslationUnit_DetailedPreprocessingRecord,
        /// Sets whether documentation comment briefs will be included in code completion results.
        pub briefs_in_completion_results: CXTranslationUnit_IncludeBriefCommentsInCodeCompletion,
        /// Sets whether the translation unit will be considered incomplete.
        ///
        /// This option suppresses certain semantic analyses and is typically used when parsing
        /// headers with the intent of creating a precompiled header.
        pub incomplete: CXTranslationUnit_Incomplete,
        /// Sets whether function and method bodies will be skipped.
        pub skip_function_bodies: CXTranslationUnit_SkipFunctionBodies,
        /// Sets whether processing will continue after a fatal error is encountered.
        #[cfg(feature="gte_clang_3_9")]
        pub keep_going: CXTranslationUnit_KeepGoing,
    }
}

impl<'tu> Parser<'tu> {
    //- Constructors -----------------------------

    fn new<F: Into<PathBuf>>(index: &'tu Index<'tu>, file: F) -> Parser<'tu> {
        let flags = CXTranslationUnit_Flags::empty();
        Parser { index: index, file: file.into(), arguments: vec![], unsaved: vec![], flags: flags }
    }

    //- Mutators ---------------------------------

    /// Sets the compiler arguments to provide to `libclang`.
    ///
    /// Any compiler argument that could be supplied to `clang` may be supplied to this
    /// function. However, the following arguments are ignored:
    ///
    /// * `-c`
    /// * `-emit-ast`
    /// * `-fsyntax-only`
    /// * `-o` and the following `<output>`
    pub fn arguments<S: AsRef<str>>(&mut self, arguments: &[S]) -> &mut Parser<'tu> {
        self.arguments = arguments.iter().map(utility::from_string).collect();
        self
    }

    /// Sets the unsaved files to use.
    pub fn unsaved(&mut self, unsaved: &[Unsaved]) -> &mut Parser<'tu> {
        self.unsaved = unsaved.into();
        self
    }

    //- Accessors --------------------------------

    /// Parses a translation unit.
    ///
    /// # Failures
    ///
    /// * an error occurs while deserializing an AST file
    /// * `libclang` crashes
    /// * an unknown error occurs
    pub fn parse(&self) -> Result<TranslationUnit<'tu>, SourceError> {
        let arguments = self.arguments.iter().map(|a| a.as_ptr()).collect::<Vec<_>>();
        let unsaved = self.unsaved.iter().map(|u| u.as_raw()).collect::<Vec<_>>();
        unsafe {
            let mut ptr = ptr::null_mut();
            let code = clang_parseTranslationUnit2(
                self.index.ptr,
                utility::from_path(&self.file).as_ptr(),
                arguments.as_ptr(),
                arguments.len() as c_int,
                mem::transmute(unsaved.as_ptr()),
                unsaved.len() as c_uint,
                self.flags,
                &mut ptr,
            );
            SourceError::from_error(code).map(|_| TranslationUnit::from_ptr(ptr))
        }
    }
}

// PlatformAvailability __________________________

/// The availability of an AST entity on a particular platform.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlatformAvailability {
    /// The name of the platform.
    pub platform: String,
    /// Whether the AST entity is unavailable on the platform.
    pub unavailable: bool,
    /// The version of the platform in which this AST entity was introduced, if any.
    pub introduced: Option<Version>,
    /// The version of the platform in which this AST entity was deprecated, if any.
    pub deprecated: Option<Version>,
    /// The version of the platform in which this AST entity was obsoleted, if any.
    pub obsoleted: Option<Version>,
    /// A message to display to users (e.g., replacement API suggestions).
    pub message: Option<String>,
}

impl PlatformAvailability {
    //- Constructors -----------------------------

    fn from_raw(mut raw: CXPlatformAvailability) -> PlatformAvailability {
        let availability = PlatformAvailability {
            platform: utility::to_string(raw.Platform),
            unavailable: raw.Unavailable != 0,
            introduced: raw.Introduced.map(Version::from_raw),
            deprecated: raw.Deprecated.map(Version::from_raw),
            obsoleted: raw.Obsoleted.map(Version::from_raw),
            message: utility::to_string_option(raw.Message),
        };
        unsafe { clang_disposeCXPlatformAvailability(&mut raw); }
        availability
    }
}

// ThreadOptions _________________________________

options! {
    /// A set of options that determines which types of threads should use background priority.
    #[derive(Default)]
    options ThreadOptions: CXGlobalOptFlags {
        /// Indicates whether threads creating for editing purposes should use background priority.
        pub editing: CXGlobalOpt_ThreadBackgroundPriorityForEditing,
        /// Indicates whether threads creating for indexing purposes should use background priority.
        pub indexing: CXGlobalOpt_ThreadBackgroundPriorityForIndexing,
    }
}

// TranslationUnit _______________________________

/// A preprocessed and parsed source file.
pub struct TranslationUnit<'i> {
    ptr: CXTranslationUnit,
    _marker: PhantomData<&'i Index<'i>>,
}

impl<'i> TranslationUnit<'i> {
    //- Constructors -----------------------------

    fn from_ptr(ptr: CXTranslationUnit) -> TranslationUnit<'i> {
        assert!(!ptr.is_null());
        TranslationUnit{ ptr: ptr, _marker: PhantomData }
    }

    /// Constructs a new `TranslationUnit` from an AST file.
    ///
    /// # Failures
    ///
    /// * an unknown error occurs
    pub fn from_ast<F: AsRef<Path>>(
        index: &'i Index, file: F
    ) -> Result<TranslationUnit<'i>, ()> {
        let path = utility::from_path(file);
        let ptr = unsafe { clang_createTranslationUnit(index.ptr, path.as_ptr()) };
        ptr.map(TranslationUnit::from_ptr).ok_or(())
    }

    //- Accessors --------------------------------

    /// Returns the diagnostics for this translation unit.
    pub fn get_diagnostics(&'i self) -> Vec<Diagnostic<'i>> {
        iter!(clang_getNumDiagnostics(self.ptr), clang_getDiagnostic(self.ptr),).map(|d| {
            Diagnostic::from_ptr(d, self)
        }).collect()
    }

    /// Returns the entity for this translation unit.
    pub fn get_entity(&'i self) -> Entity<'i> {
        unsafe { Entity::from_raw(clang_getTranslationUnitCursor(self.ptr), self) }
    }

    /// Returns the file at the supplied path in this translation unit, if any.
    pub fn get_file<F: AsRef<Path>>(&'i self, file: F) -> Option<File<'i>> {
        let file = unsafe { clang_getFile(self.ptr, utility::from_path(file).as_ptr()) };
        file.map(|f| File::from_ptr(f, self))
    }

    /// Returns the memory usage of this translation unit.
    pub fn get_memory_usage(&self) -> HashMap<MemoryUsage, usize> {
        unsafe {
            let raw = clang_getCXTUResourceUsage(self.ptr);
            let raws = slice::from_raw_parts(raw.entries, raw.numEntries as usize);
            let usage = raws.iter().map(|u| (mem::transmute(u.kind), u.amount as usize)).collect();
            clang_disposeCXTUResourceUsage(raw);
            usage
        }
    }

    /// Returns the AST entities which correspond to the supplied tokens, if any.
    pub fn annotate(&'i self, tokens: &[Token<'i>]) -> Vec<Option<Entity<'i>>> {
        unsafe {
            let mut raws = vec![CXCursor::default(); tokens.len()];
            let ptr = mem::transmute(tokens.as_ptr());
            clang_annotateTokens(self.ptr, ptr, tokens.len() as c_uint, raws.as_mut_ptr());
            raws.iter().map(|e| e.map(|e| Entity::from_raw(e, self))).collect()
        }
    }

    /// Returns a completer which runs code completion.
    pub fn completer<F: Into<PathBuf>>(&self, file: F, line: u32, column: u32) -> Completer {
        Completer::new(self, file, line, column)
    }

    /// Saves this translation unit to an AST file.
    ///
    /// # Failures
    ///
    /// * errors in the translation unit prevent saving
    /// * an unknown error occurs
    pub fn save<F: AsRef<Path>>(&self, file: F) -> Result<(), SaveError> {
        let file = utility::from_path(file);
        let flags = CXSaveTranslationUnit_None;
        let code = unsafe { clang_saveTranslationUnit(self.ptr, file.as_ptr(), flags) };
        SaveError::from_error(code)
    }

    //- Consumers --------------------------------

    /// Consumes this translation unit and reparses the source file it was created from with the
    /// same compiler arguments that were used originally.
    ///
    /// # Failures
    ///
    /// * an error occurs while deserializing an AST file
    /// * `libclang` crashes
    /// * an unknown error occurs
    pub fn reparse(self, unsaved: &[Unsaved]) -> Result<TranslationUnit<'i>, SourceError> {
        let unsaved = unsaved.iter().map(|u| u.as_raw()).collect::<Vec<_>>();
        unsafe {
            let code = clang_reparseTranslationUnit(
                self.ptr,
                unsaved.len() as c_uint,
                mem::transmute(unsaved.as_ptr()),
                CXReparse_None,
            );
            SourceError::from_error(code).map(|_| self)
        }
    }
}

impl<'i> Drop for TranslationUnit<'i> {
    fn drop(&mut self) {
        unsafe { clang_disposeTranslationUnit(self.ptr); }
    }
}

impl<'i> fmt::Debug for TranslationUnit<'i> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let spelling = unsafe { clang_getTranslationUnitSpelling(self.ptr) };
        formatter.debug_struct("TranslationUnit")
            .field("spelling", &utility::to_string(spelling))
            .finish()
    }
}

// Type __________________________________________

/// The type of an AST entity.
#[derive(Copy, Clone)]
pub struct Type<'tu> {
    raw: CXType,
    tu: &'tu TranslationUnit<'tu>,
}

impl<'tu> Type<'tu> {
    //- Constructors -----------------------------

    fn from_raw(raw: CXType, tu: &'tu TranslationUnit<'tu>) -> Type<'tu> {
        Type { raw: raw, tu: tu }
    }

    //- Accessors --------------------------------

    /// Returns the kind of this type.
    pub fn get_kind(&self) -> TypeKind {
        unsafe { mem::transmute(self.raw.kind) }
    }

    /// Returns the display name of this type.
    pub fn get_display_name(&self) -> String {
        unsafe { utility::to_string(clang_getTypeSpelling(self.raw)) }
    }

    /// Returns the alignment of this type in bytes.
    ///
    /// # Failures
    ///
    /// * this type is a dependent type
    /// * this type is an incomplete type
    pub fn get_alignof(&self) -> Result<usize, AlignofError> {
        let alignof_ = unsafe { clang_Type_getAlignOf(self.raw) };
        AlignofError::from_error(alignof_).map(|_| alignof_ as usize)
    }

    /// Returns the offset of the field with the supplied name in this record type in bits.
    ///
    /// # Failures
    ///
    /// * this record type is a dependent type
    /// * this record record type is an incomplete type
    /// * this record type does not contain a field with the supplied name
    pub fn get_offsetof<F: AsRef<str>>(&self, field: F) -> Result<usize, OffsetofError> {
        let field = utility::from_string(field);
        let offsetof_ = unsafe { clang_Type_getOffsetOf(self.raw, field.as_ptr()) };
        OffsetofError::from_error(offsetof_).map(|_| offsetof_ as usize)
    }

    /// Returns the size of this type in bytes.
    ///
    /// # Failures
    ///
    /// * this type is a dependent type
    /// * this type is an incomplete type
    /// * this type is a variable size type
    pub fn get_sizeof(&self) -> Result<usize, SizeofError> {
        let sizeof_ = unsafe { clang_Type_getSizeOf(self.raw) };
        SizeofError::from_error(sizeof_).map(|_| sizeof_ as usize)
    }

    /// Returns the argument types for this function or method type, if applicable.
    pub fn get_argument_types(&self) -> Option<Vec<Type<'tu>>> {
        iter_option!(
            clang_getNumArgTypes(self.raw),
            clang_getArgType(self.raw),
        ).map(|i| i.map(|t| Type::from_raw(t, self.tu)).collect())
    }

    /// Returns the calling convention specified for this function type, if applicable.
    pub fn get_calling_convention(&self) -> Option<CallingConvention> {
        unsafe {
            match clang_getFunctionTypeCallingConv(self.raw) {
                CXCallingConv_Invalid => None,
                other => Some(mem::transmute(other)),
            }
        }
    }

    /// Returns the canonical type for this type.
    ///
    /// The canonical type is the underlying type with all "sugar" removed (e.g., typedefs).
    pub fn get_canonical_type(&self) -> Type<'tu> {
        unsafe { Type::from_raw(clang_getCanonicalType(self.raw), self.tu) }
    }

    /// Returns the class type for this member pointer type, if applicable.
    pub fn get_class_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_Type_getClassType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the AST entity that declared this type, if any.
    pub fn get_declaration(&self) -> Option<Entity<'tu>> {
        unsafe { clang_getTypeDeclaration(self.raw).map(|e| Entity::from_raw(e, self.tu)) }
    }

    /// Returns the type named by this elaborated type, if applicable.
    #[cfg(feature="gte_clang_3_9")]
    pub fn get_elaborated_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_Type_getNamedType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the element type for this array, complex, or vector type, if applicable.
    pub fn get_element_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_getElementType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the fields in this record type, if applicable.
    #[cfg(feature="gte_clang_3_7")]
    pub fn get_fields(&self) -> Option<Vec<Entity<'tu>>> {
        if self.get_kind() == TypeKind::Record {
            let mut fields = vec![];
            self.visit_fields(|e| {
                fields.push(e);
                true
            });
            Some(fields)
        } else {
            None
        }
    }

    /// Returns the encoding of this Objective-C type, if applicable.
    #[cfg(feature="gte_clang_3_9")]
    pub fn get_objc_encoding(&self) -> Option<String> {
        unsafe { utility::to_string_option(clang_Type_getObjCEncoding(self.raw)) }
    }

    /// Returns the pointee type for this pointer type, if applicable.
    pub fn get_pointee_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_getPointeeType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the ref qualifier for this C++ function or method type, if applicable.
    pub fn get_ref_qualifier(&self) -> Option<RefQualifier> {
        unsafe {
            match clang_Type_getCXXRefQualifier(self.raw) {
                CXRefQualifier_None => None,
                other => Some(mem::transmute(other)),
            }
        }
    }

    /// Returns the result type for this function or method type, if applicable.
    pub fn get_result_type(&self) -> Option<Type<'tu>> {
        unsafe { clang_getResultType(self.raw).map(|t| Type::from_raw(t, self.tu)) }
    }

    /// Returns the size of this constant array or vector type, if applicable.
    pub fn get_size(&self) -> Option<usize> {
        let size = unsafe { clang_getNumElements(self.raw) };
        if size >= 0 {
            Some(size as usize)
        } else {
            None
        }
    }

    /// Returns the template argument types for this template class specialization type, if
    /// applicable.
    pub fn get_template_argument_types(&self) -> Option<Vec<Option<Type<'tu>>>> {
        iter_option!(
            clang_Type_getNumTemplateArguments(self.raw),
            clang_Type_getTemplateArgumentAsType(self.raw),
        ).map(|i| i.map(|t| t.map(|t| Type::from_raw(t, self.tu))).collect())
    }

    /// Returns whether this type is qualified with const.
    pub fn is_const_qualified(&self) -> bool {
        unsafe { clang_isConstQualifiedType(self.raw) != 0 }
    }

    /// Returns whether this type is an elaborated type, if it can be determined for certain.
    pub fn is_elaborated(&self) -> Option<bool> {
        if unsafe { mem::transmute::<_, c_int>(self.raw.kind) } == 119 {
            Some(true)
        } else if cfg!(feature="gte_clang_3_9") {
            Some(false)
        } else {
            None
        }
    }

    /// Returns whether this type is plain old data (POD).
    pub fn is_pod(&self) -> bool {
        unsafe { clang_isPODType(self.raw) != 0 }
    }

    /// Returns whether this type is qualified with restrict.
    pub fn is_restrict_qualified(&self) -> bool {
        unsafe { clang_isRestrictQualifiedType(self.raw) != 0 }
    }

    /// Returns whether this type is a variadic function type.
    pub fn is_variadic(&self) -> bool {
        unsafe { clang_isFunctionTypeVariadic(self.raw) != 0 }
    }

    /// Returns whether this type is qualified with volatile.
    pub fn is_volatile_qualified(&self) -> bool {
        unsafe { clang_isVolatileQualifiedType(self.raw) != 0 }
    }

    /// Visits the fields in this record type, returning `None` if this type is not a record type
    /// and returning `Some(b)` otherwise where `b` indicates whether visitation was ended by the
    /// callback returning `false`.
    #[cfg(feature="gte_clang_3_7")]
    pub fn visit_fields<F: FnMut(Entity<'tu>) -> bool>(&self, f: F) -> Option<bool> {
        if self.get_kind() != TypeKind::Record {
            return None;
        }

        trait Callback<'tu> {
            fn call(&mut self, field: Entity<'tu>) -> bool;
        }

        impl<'tu, F: FnMut(Entity<'tu>) -> bool> Callback<'tu> for F {
            fn call(&mut self, field: Entity<'tu>) -> bool {
                self(field)
            }
        }

        extern fn visit(cursor: CXCursor, data: CXClientData) -> CXVisitorResult {
            unsafe {
                let &mut (tu, ref mut callback) =
                    &mut *(data as *mut (&TranslationUnit, Box<Callback>));

                if callback.call(Entity::from_raw(cursor, tu)) {
                    CXVisit_Continue
                } else {
                    CXVisit_Break
                }
            }
        }

        let mut data = (self.tu, Box::new(f) as Box<Callback>);
        unsafe {
            let data = utility::addressof(&mut data);
            Some(clang_Type_visitFields(self.raw, visit, data) == CXVisit_Break)
        }
    }

    //- Categorization ---------------------------

    /// Returns whether this type is an integer type.
    pub fn is_integer(&self) -> bool {
        self.raw.kind >= CXType_Bool && self.raw.kind <= CXType_Int128
    }

    /// Returns whether this type is a signed integer type.
    pub fn is_signed_integer(&self) -> bool {
        self.raw.kind >= CXType_Char_S && self.raw.kind <= CXType_Int128
    }

    /// Returns whether this type is an unsigned integer type.
    pub fn is_unsigned_integer(&self) -> bool {
        self.raw.kind >= CXType_Bool && self.raw.kind <= CXType_UInt128
    }
}

impl<'tu> fmt::Debug for Type<'tu> {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.debug_struct("Type")
            .field("kind", &self.get_kind())
            .field("display_name", &self.get_display_name())
            .finish()
    }
}

impl<'tu> cmp::PartialEq for Type<'tu> {
    fn eq(&self, other: &Type<'tu>) -> bool {
        unsafe { clang_equalTypes(self.raw, other.raw) != 0 }
    }
}

impl<'tu> cmp::Eq for Type<'tu> { }

// Unsaved _______________________________________

/// The path to and unsaved contents of a previously existing file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Unsaved {
    path: CString,
    contents: CString,
}

impl Unsaved {
    //- Constructors -----------------------------

    /// Constructs a new `Unsaved`.
    pub fn new<P: AsRef<Path>, C: AsRef<str>>(path: P, contents: C) -> Unsaved {
        Unsaved { path: utility::from_path(path), contents: utility::from_string(contents) }
    }

    //- Accessors --------------------------------

    fn as_raw(&self) -> CXUnsavedFile {
        CXUnsavedFile {
            Filename: self.path.as_ptr(),
            Contents: self.contents.as_ptr(),
            Length: self.contents.as_bytes().len() as c_ulong,
        }
    }
}

// Usr ___________________________________________

/// A Unified Symbol Resolution (USR).
///
/// A USR identifies an AST entity and can be used to compare AST entities from different
/// translation units.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Usr(pub String);

impl Usr {
    //- Constructors -----------------------------

    /// Constructs a new `Usr` from an Objective-C category.
    pub fn from_objc_category<C: AsRef<str>>(class: C, category: C) -> Usr {
        let class = utility::from_string(class);
        let category = utility::from_string(category);
        let raw = unsafe { clang_constructUSR_ObjCCategory(class.as_ptr(), category.as_ptr()) };
        Usr(utility::to_string(raw))
    }

    /// Constructs a new `Usr` from an Objective-C class.
    pub fn from_objc_class<C: AsRef<str>>(class: C) -> Usr {
        let class = utility::from_string(class);
        unsafe { Usr(utility::to_string(clang_constructUSR_ObjCClass(class.as_ptr()))) }
    }

    /// Constructs a new `Usr` from an Objective-C instance variable.
    pub fn from_objc_ivar<N: AsRef<str>>(class: &Usr, name: N) -> Usr {
        utility::with_string(&class.0, |s| {
            let name = utility::from_string(name);
            unsafe { Usr(utility::to_string(clang_constructUSR_ObjCIvar(name.as_ptr(), s))) }
        })
    }

    /// Constructs a new `Usr` from an Objective-C method.
    pub fn from_objc_method<N: AsRef<str>>(class: &Usr, name: N, instance: bool) -> Usr {
        utility::with_string(&class.0, |s| {
            let name = utility::from_string(name);
            let instance = instance as c_uint;
            let raw = unsafe { clang_constructUSR_ObjCMethod(name.as_ptr(), instance, s) };
            Usr(utility::to_string(raw))
        })
    }

    /// Constructs a new `Usr` from an Objective-C property.
    pub fn from_objc_property<N: AsRef<str>>(class: &Usr, name: N) -> Usr {
        utility::with_string(&class.0, |s| {
            let name = utility::from_string(name);
            unsafe { Usr(utility::to_string(clang_constructUSR_ObjCProperty(name.as_ptr(), s))) }
        })
    }

    /// Constructs a new `Usr` from an Objective-C protocol.
    pub fn from_objc_protocol<P: AsRef<str>>(protocol: P) -> Usr {
        let string = utility::from_string(protocol);
        unsafe { Usr(utility::to_string(clang_constructUSR_ObjCProtocol(string.as_ptr()))) }
    }
}

// Version _______________________________________

/// A version number in the form `x.y.z`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    /// The `x` component of the version number.
    pub x: i32,
    /// The `y` component of the version number.
    pub y: i32,
    /// The `z` component of the version number.
    pub z: i32,
}

impl Version {
    //- Constructors -----------------------------

    fn from_raw(raw: CXVersion) -> Version {
        Version { x: raw.Major as i32, y: raw.Minor as i32, z: raw.Subminor as i32 }
    }
}

//================================================
// Functions
//================================================

/// Returns the version string for the version of `libclang` in use.
pub fn get_version() -> String {
    unsafe { utility::to_string(clang_getClangVersion()) }
}
