// SPDX-License-Identifier: MPL-2.0
//! Invalidação local x86-64; não qualifica shootdown remoto ou execução de módulos.
#![forbid(unsafe_code)]

use super::{OpsStack, TlbFlushOp};
use crate::{
    mm::{PAGE_SIZE, kspace::module_staging_map::probe_global_flush_for_test},
    prelude::*,
};

#[ktest]
fn cybermanju_tlb_global_runtime() {
    assert_eq!(crate::cpu::num_cpus(), 1);
    let base = PAGE_SIZE * 16;
    // Quatro casos de representação/contrato, incluindo o limite de 32 páginas.
    assert_eq!(
        core::mem::size_of::<TlbFlushOp>(),
        core::mem::size_of::<usize>()
    );
    assert_ne!(
        TlbFlushOp::for_all(),
        TlbFlushOp::for_all_including_global()
    );
    assert_eq!(
        TlbFlushOp::for_range(base..base + 32 * PAGE_SIZE),
        TlbFlushOp::for_all_including_global()
    );
    assert_eq!(
        TlbFlushOp::for_range(base..base + 31 * PAGE_SIZE).num_pages(),
        31
    );
    println!("\nCYBERMANJU_TLB_GLOBAL:encoding:PASS cases=4");

    let mut stack = OpsStack::new();
    assert!(stack.is_empty());
    assert!(!stack.need_flush_global); // 1: fila vazia.
    stack.push(TlbFlushOp::for_single(base), None);
    assert_eq!((stack.num_ops, stack.num_pages_to_flush), (1, 1)); // 2.
    for i in 1..31 {
        stack.push(TlbFlushOp::for_single(base + i * PAGE_SIZE), None);
    }
    assert_eq!((stack.num_ops, stack.num_pages_to_flush), (31, 31)); // 3.
    stack.push(TlbFlushOp::for_single(base + 31 * PAGE_SIZE), None);
    assert!(stack.need_flush_all());
    assert_eq!(stack.num_ops, 0); // 4: promoção no limite.
    stack.clear_without_flush();
    assert!(stack.is_empty());
    assert!(!stack.need_flush_global); // 5: reset sem liberar frames nesta prova.
    stack.push(TlbFlushOp::for_all(), None);
    assert!(!stack.need_flush_global);
    stack.push(TlbFlushOp::for_single(base), None);
    assert!(stack.need_flush_global);
    assert!(stack.need_flush_all()); // 6: escopo misto.
    let mut full = OpsStack::new();
    let mut specific = OpsStack::new();
    full.push(TlbFlushOp::for_all(), None);
    specific.push(TlbFlushOp::for_single(base), None);
    full.push_from(&specific);
    assert!(full.need_flush_all() && full.need_flush_global);
    specific.clear_without_flush();
    specific.push(TlbFlushOp::for_all(), None);
    let mut inverse = OpsStack::new();
    inverse.push(TlbFlushOp::for_single(base), None);
    inverse.push_from(&specific);
    assert!(inverse.need_flush_all() && inverse.need_flush_global);
    stack.clear_without_flush();
    stack.push(TlbFlushOp::for_single(base), None);
    stack.push(TlbFlushOp::for_all_including_global(), None);
    assert!(stack.need_flush_all()); // 7: sentinela sem overflow de contador.
    stack.clear_without_flush();
    let mut other = OpsStack::new();
    for i in 0..16 {
        stack.push(TlbFlushOp::for_single(base + i * PAGE_SIZE), None);
        other.push(TlbFlushOp::for_single(base + (16 + i) * PAGE_SIZE), None);
    }
    stack.push_from(&other);
    assert!(stack.need_flush_all());
    assert_eq!(stack.num_ops, 0);
    assert!(stack.need_flush_global); // 8: agregação de filas.
    println!("CYBERMANJU_TLB_GLOBAL:aggregation:PASS cases=8");

    for case in 0..7 {
        let observed = probe_global_flush_for_test(|range| match case {
            0 => TlbFlushOp::for_range(range.start..range.start + 31 * PAGE_SIZE)
                .perform_on_current(),
            1 => TlbFlushOp::for_range(range.start..range.start + 32 * PAGE_SIZE)
                .perform_on_current(),
            2 => {
                let mut ops = OpsStack::new();
                for i in 0..32 {
                    ops.push(TlbFlushOp::for_single(range.start + i * PAGE_SIZE), None);
                }
                ops.flush_all();
            }
            3 => {
                let mut left = OpsStack::new();
                let mut right = OpsStack::new();
                for i in 0..16 {
                    left.push(TlbFlushOp::for_single(range.start + i * PAGE_SIZE), None);
                    right.push(
                        TlbFlushOp::for_single(range.start + (16 + i) * PAGE_SIZE),
                        None,
                    );
                }
                left.push_from(&right);
                left.flush_all();
            }
            4 => {
                let mut ops = OpsStack::new();
                ops.push(TlbFlushOp::for_all(), None);
                ops.push(TlbFlushOp::for_single(range.start), None);
                ops.flush_all();
            }
            5 => {
                let mut ops = OpsStack::new();
                ops.push(TlbFlushOp::for_single(range.start), None);
                ops.push(TlbFlushOp::for_all(), None);
                ops.flush_all();
            }
            6 => TlbFlushOp::for_all_including_global().perform_on_current(),
            _ => unreachable!(),
        });
        assert_eq!(
            observed, [0x42; 8],
            "tradução GLOBAL estrangeira sobreviveu no caso {}",
            case
        );
    }
    println!("CYBERMANJU_TLB_GLOBAL:stale_readback:PASS cases=7 cpus=1");
}
