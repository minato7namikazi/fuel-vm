use super::{ExecuteError, Interpreter};
use crate::consts::{MEM_MAX_ACCESS_SIZE, VM_MAX_RAM};
use crate::crypto;

use fuel_asm::Word;
use fuel_tx::crypto::Hasher;
use fuel_tx::{Bytes32, Bytes64};

impl<S> Interpreter<S> {
    pub(crate) fn ecrecover(&mut self, a: Word, b: Word, c: Word) -> Result<(), ExecuteError> {
        if a > VM_MAX_RAM - Bytes64::size_of() as Word
            || b > VM_MAX_RAM - Bytes64::size_of() as Word
            || c > VM_MAX_RAM - Bytes32::size_of() as Word
        {
            return Err(ExecuteError::MemoryOverflow);
        }

        let (a, b, c) = (a as usize, b as usize, c as usize);

        let bx = b + Bytes64::size_of();
        let cx = c + Bytes32::size_of();

        let e = &self.memory[c..cx];
        let sig = &self.memory[b..bx];

        match crypto::secp256k1_sign_compact_recover(sig, e) {
            Ok(pk) => {
                self.try_mem_write(a, pk.as_ref())?;
                self.clear_err();
            }

            Err(_) => {
                self.try_zeroize(a, Bytes64::size_of())?;
                self.set_err();
            }
        }

        self.inc_pc();

        Ok(())
    }

    pub(crate) fn keccak256(&mut self, a: Word, b: Word, c: Word) -> Result<(), ExecuteError> {
        use sha3::{Digest, Keccak256};

        if a > VM_MAX_RAM - Bytes32::size_of() as Word || c > MEM_MAX_ACCESS_SIZE || b > VM_MAX_RAM - c {
            return Err(ExecuteError::MemoryOverflow);
        }

        let (a, b, c) = (a as usize, b as usize, c as usize);
        let bc = b + c;

        let mut h = Keccak256::new();

        h.update(&self.memory[b..bc]);

        self.try_mem_write(a, h.finalize().as_slice())?;

        self.inc_pc();

        Ok(())
    }

    pub(crate) fn sha256(&mut self, a: Word, b: Word, c: Word) -> Result<(), ExecuteError> {
        if a > VM_MAX_RAM - Bytes32::size_of() as Word || c > MEM_MAX_ACCESS_SIZE || b > VM_MAX_RAM - c {
            return Err(ExecuteError::MemoryOverflow);
        }

        let (a, b, c) = (a as usize, b as usize, c as usize);
        let bc = b + c;

        self.try_mem_write(a, Hasher::hash(&self.memory[b..bc]).as_ref())?;

        self.inc_pc();

        Ok(())
    }
}