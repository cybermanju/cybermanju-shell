// SPDX-License-Identifier: MPL-2.0
//! Helpers restritos a ktest; não expõem configuração de CPU em produção.

use x86_64::registers::control::{Cr4, Cr4Flags};

use crate::irq::{self, DisabledLocalIrqGuard};

/// Mantém IRQ desabilitada enquanto um teste modifica PGE nesta CPU.
/// Restaura PGE antes de restaurar o estado anterior de IRQ, inclusive em panic.
pub(crate) struct TestPgeGuard {
    original_pge: bool,
    _irq_guard: DisabledLocalIrqGuard,
}

impl TestPgeGuard {
    pub(crate) fn new(enabled: bool) -> Self {
        let irq_guard = irq::disable_local();
        let original_pge = Cr4::read().contains(Cr4Flags::PAGE_GLOBAL);
        // SAFETY: somente PGE muda, invalidando traduções; CPU fixada pelo guard.
        unsafe {
            Cr4::update(|flags| flags.set(Cr4Flags::PAGE_GLOBAL, enabled));
        }
        Self {
            original_pge,
            _irq_guard: irq_guard,
        }
    }
}

impl Drop for TestPgeGuard {
    fn drop(&mut self) {
        // SAFETY: restaura somente PGE, ainda com IRQ local desabilitada.
        unsafe {
            Cr4::update(|flags| flags.set(Cr4Flags::PAGE_GLOBAL, self.original_pge));
        }
    }
}

pub(crate) fn assert_flush_preserves_cpu_state() {
    assert_eq!(crate::cpu::num_cpus(), 1);
    // Esta entrada deve ser o contexto normal habilitado, não um guard externo.
    assert!(
        irq::is_local_enabled(),
        "a prova exige IRQ habilitada na entrada"
    );
    let before = Cr4::read_raw();
    super::tlb_flush_all_including_global();
    assert!(
        irq::is_local_enabled(),
        "flush deve restaurar IRQ habilitada"
    );
    assert_eq!(
        Cr4::read_raw(),
        before,
        "flush deve preservar CR4 na entrada normal"
    );

    for enabled in [false, true] {
        let _pge = TestPgeGuard::new(enabled);
        let before = Cr4::read_raw();
        assert_eq!(Cr4::read().contains(Cr4Flags::PAGE_GLOBAL), enabled);
        assert!(!irq::is_local_enabled());
        super::tlb_flush_all_including_global();
        assert!(
            !irq::is_local_enabled(),
            "flush aninhado não pode habilitar IRQ"
        );
        assert_eq!(
            Cr4::read_raw(),
            before,
            "flush deve preservar CR4/PGE em ambos os estados"
        );
    }
}
