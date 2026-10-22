// SPDX-License-Identifier: MPL-2.0
//! Dispatch/ack de TLB em duas CPUs; não prova coerência de traduções ou execução.
#![forbid(unsafe_code)]

use core::sync::atomic::Ordering;

use super::{
    COMPLETED_BATCHES, COMPLETED_GLOBAL_BATCHES, IRQ_DISABLED_BATCHES, TlbFlushOp, TlbFlusher,
};
use crate::{
    cpu::{self, AtomicCpuSet, CpuId, CpuSet, PinCurrentCpu},
    irq,
    mm::PAGE_SIZE,
    prelude::*,
    task::disable_preempt,
    util::id_set::Id,
};

/// Invocado só em ktest, depois da invalidação de uma fila não vazia.
pub(super) fn record_completed(full_global: bool) {
    let irq_was_disabled = !irq::is_local_enabled();
    let guard = irq::disable_local();
    let cpu = guard.current_cpu();
    if full_global {
        COMPLETED_GLOBAL_BATCHES
            .get_on_cpu(cpu)
            .fetch_add(1, Ordering::Release);
    }
    if irq_was_disabled {
        IRQ_DISABLED_BATCHES
            .get_on_cpu(cpu)
            .fetch_add(1, Ordering::Release);
    }
    COMPLETED_BATCHES
        .get_on_cpu(cpu)
        .fetch_add(1, Ordering::Release);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Snapshot {
    batches: usize,
    global: usize,
    irq_disabled: usize,
}

fn snapshot(cpu: CpuId) -> Snapshot {
    Snapshot {
        batches: COMPLETED_BATCHES.get_on_cpu(cpu).load(Ordering::Acquire),
        global: COMPLETED_GLOBAL_BATCHES
            .get_on_cpu(cpu)
            .load(Ordering::Acquire),
        irq_disabled: IRQ_DISABLED_BATCHES.get_on_cpu(cpu).load(Ordering::Acquire),
    }
}

fn expect(cpu: CpuId, before: Snapshot, batches: usize, global: usize) {
    assert_eq!(
        snapshot(cpu),
        Snapshot {
            batches: before.batches + batches,
            global: before.global + global,
            irq_disabled: before.irq_disabled + batches,
        }
    );
}

#[ktest]
fn cybermanju_tlb_remote_runtime() {
    assert_eq!(cpu::num_cpus(), 2, "a prova exige duas CPUs reais na VM");
    assert!(
        crate::smp::IPI_SENDER.get().is_some(),
        "IPI sender deve existir"
    );
    assert!(
        irq::is_local_enabled(),
        "não esperar IPIs com IRQ desabilitada"
    );
    let pin = disable_preempt();
    let source = pin.current_cpu();
    let target = cpu::all_cpus().find(|cpu| *cpu != source).unwrap();
    let targets = AtomicCpuSet::new(CpuSet::from(target));
    let both = AtomicCpuSet::new(CpuSet::new_full());
    let mut remote = TlbFlusher::new(&targets, disable_preempt());
    let mut mixed = TlbFlusher::new(&both, disable_preempt());
    // Drena trabalho IPI anterior antes do baseline; suite filtrada e sem workload.
    crate::smp::inter_processor_call(&CpuSet::new_full(), || {}).wait();
    // IDs e contadores são lidos do CPU em que a fila realmente foi drenada.
    let initial_local = snapshot(source);
    let initial_remote = snapshot(target);
    println!(
        "\nCYBERMANJU_TLB_REMOTE:topology:PASS cpus=2 source={} target={}",
        source.as_usize(),
        target.as_usize()
    );

    // Fila vazia não gera trabalho remoto; sync não despacha pedidos pendentes.
    remote.dispatch_tlb_flush();
    remote.sync_tlb_flush();
    expect(source, initial_local, 0, 0);
    expect(target, initial_remote, 0, 0);
    let address = PAGE_SIZE * 16; // INVLPG de VA canônico; não instala mapping.
    remote.issue_tlb_flush(TlbFlushOp::for_single(address));
    remote.sync_tlb_flush();
    expect(source, initial_local, 0, 0);
    expect(target, initial_remote, 0, 0);

    // 1. Pedido remoto apenas; sender não deve executar a fila.
    remote.dispatch_tlb_flush();
    remote.sync_tlb_flush();
    expect(source, initial_local, 0, 0);
    expect(target, initial_remote, 1, 0);
    // 2. Mesmo flusher reutilizado com operação inclusiva explícita.
    remote.issue_tlb_flush(TlbFlushOp::for_all_including_global());
    remote.dispatch_tlb_flush();
    remote.sync_tlb_flush();
    expect(source, initial_local, 0, 0);
    expect(target, initial_remote, 2, 1);
    // 3. Seleção mista: uma execução em cada CPU, incluindo o sender.
    mixed.issue_tlb_flush(TlbFlushOp::for_single(address));
    mixed.dispatch_tlb_flush();
    mixed.sync_tlb_flush();
    expect(source, initial_local, 1, 0);
    expect(target, initial_remote, 3, 1);
    // 4. Promoção de range de 32 páginas chega como flush GLOBAL remoto.
    remote.issue_tlb_flush(TlbFlushOp::for_range(address..address + 32 * PAGE_SIZE));
    remote.dispatch_tlb_flush();
    remote.sync_tlb_flush();
    expect(source, initial_local, 1, 0);
    expect(target, initial_remote, 4, 2);
    // A segunda espera não deve reenviar nem executar trabalho adicional.
    remote.sync_tlb_flush();
    mixed.sync_tlb_flush();
    expect(source, initial_local, 1, 0);
    expect(target, initial_remote, 4, 2);
    assert!(irq::is_local_enabled());
    println!(
        "CYBERMANJU_TLB_REMOTE:dispatch:PASS cases=4 remote_batches=4 local_batches=1 global_batches=2"
    );
    println!(
        "CYBERMANJU_TLB_REMOTE:lifecycle:PASS empty_cases=2 sync_reused=true maps_installed=false init_called=false"
    );
}
