//! FuelVM implementation

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(unused_crate_dependencies)]
#![deny(
    clippy::arithmetic_side_effects,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::string_slice
)]

#[doc(hidden)] // Needed by some of the exported macros
pub extern crate alloc;

extern crate core;
#[cfg(feature = "std")]
extern crate libm as _; // Not needed with stdlib

#[cfg(test)]
use criterion as _;

pub mod backtrace;
pub mod call;
pub mod checked_transaction;
pub mod constraints;
pub mod consts;
pub mod context;
mod convert;
pub mod crypto;
pub mod error;
pub mod interpreter;
#[cfg(feature = "test-helpers")]
pub mod memory_client;
pub mod pool;
pub mod predicate;
pub mod state;
pub mod storage;
pub mod transactor;
pub mod util;
pub mod verification;

#[cfg(test)]
mod tests;

// Fully re-export fuel dependencies
#[doc(no_inline)]
pub use fuel_asm;
#[doc(no_inline)]
#[cfg(feature = "da-compression")]
pub use fuel_compression;
#[doc(no_inline)]
pub use fuel_crypto;
#[doc(no_inline)]
pub use fuel_merkle;
#[doc(no_inline)]
pub use fuel_storage;
#[doc(no_inline)]
pub use fuel_tx;
#[doc(no_inline)]
pub use fuel_types;

pub mod prelude {
    //! Required implementations for full functionality
    #[doc(no_inline)]
    pub use fuel_asm::{
        GMArgs,
        GTFArgs,
        Instruction,
        Opcode,
        PanicReason,
        RegId,
    };
    #[doc(no_inline)]
    pub use fuel_crypto::{
        Hasher,
        Message,
        PublicKey,
        SecretKey,
        Signature,
    };
    #[doc(no_inline)]
    pub use fuel_storage::{
        MerkleRoot,
        MerkleRootStorage,
        StorageAsMut,
        StorageAsRef,
        StorageInspect,
        StorageMutate,
    };
    #[doc(no_inline)]
    pub use fuel_tx::*;
    #[doc(no_inline)]
    pub use fuel_types::{
        Address,
        AssetId,
        BlobId,
        Bytes4,
        Bytes8,
        Bytes32,
        Bytes64,
        ContractId,
        Immediate06,
        Immediate12,
        Immediate18,
        Immediate24,
        RegisterId,
        Salt,
        Word,
    };

    pub use crate::{
        backtrace::Backtrace,
        call::{
            Call,
            CallFrame,
        },
        context::Context,
        error::{
            Bug,
            BugVariant,
            InterpreterError,
            RuntimeError,
        },
        interpreter::{
            ExecutableTransaction,
            Interpreter,
            Memory,
            MemoryInstance,
            MemoryRange,
            predicates,
        },
        pool::VmMemoryPool,
        predicate::RuntimePredicate,
        state::{
            Debugger,
            ProgramState,
            StateTransition,
            StateTransitionRef,
        },
        storage::{
            InterpreterStorage,
            predicate::PredicateStorage,
        },
        transactor::Transactor,
    };

    pub use crate::state::{
        Breakpoint,
        DebugEval,
    };

    #[cfg(any(test, feature = "test-helpers"))]
    pub use crate::{
        checked_transaction::{
            IntoChecked,
            builder::TransactionBuilderExt,
        },
        memory_client::MemoryClient,
        storage::MemoryStorage,
        util::test_helpers::TestBuilder,
    };
}
