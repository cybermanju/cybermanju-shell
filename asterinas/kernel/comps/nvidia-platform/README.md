# Asterinas NVIDIA platform inventory (experimental)

`aster-nvidia-platform` is a small, read-only layer over the existing `aster-pci` framework in `kernel/core/comps/pci`. It does **not** pretend that PCI support is absent: `PciCommonDevice` exposes the real `PciDeviceId`, location, and `BarManager`; the PCI framework has already enumerated the function and populated BAR metadata.

## API

- `ResourceSnapshot::from_device(&PciCommonDevice)` accepts vendor `0x10de` and PCI base class `0x03` (display controller), then reports each existing BAR's framework-provided address, size, memory/I/O kind, memory width, and prefetchability. It requires a valid memory BAR0 and rejects zero ranges and address overflow.
- `ConfigReader::new(&device)` provides bounded, read-only 8/16/32-bit config reads within the conventional 256-byte config space. It checks bounds/alignment before using the public `PciDeviceLocation` read methods; it does not invent a write interface. The underlying `aster-pci` accessor is infallible/panic-on-backend-error, so this wrapper is intended for an already enumerated function and cannot turn hardware errors into recoverable results.
- `resource_inventory()` borrows unclaimed NVIDIA display functions from the initialized `PCI_BUS`, snapshots their actual enumerator metadata, and does not remove them from the bus. Call only after PCI component initialization.

The snapshot does not call `Bar::acquire`, resize BARs, map MMIO, claim/reserve a resource, change PCI command bits, or construct a `PciDriver`. Its output is diagnostic metadata only—not a bound/initialized GPU, a successful probe, or working NVIDIA RM. BAR lengths are copied from `BarManager`'s measured `MemoryBar::size()` / `IoBar::size()` values, never inferred from addresses. Note that Asterinas' PCI enumerator itself performs the PCI-spec BAR sizing transaction while constructing the manager; this new crate merely reads its resulting metadata.

The crate is a workspace/default member and is optionally linked into the kernel by `nvidia-pci-diagnostics` (also enabled by `native-gpu-ci`). Its component runs after its PCI dependency and logs actual resources or explicitly says no unclaimed NVIDIA display controller was found. Component initialization returning `Ok(())` means the diagnostic ran, not that a GPU driver initialized; no `PciDriver` is registered.

## Native callback worker

`worker::DeviceWorker` owns one real OSTD task and one callback. `WorkSignal::schedule` coalesces requests while pending; a request during callback execution installs one subsequent invocation. `DeviceWorker::flush` takes a lock-protected snapshot of accepted invocation generations and waits only through that snapshot. Coalesced pending requests count as one invocation, while requests accepted after the snapshot do not extend it. Flush is sleepable-task-only, rejects self-wait, does not close admission, and does not destroy callback captures; use `shutdown` when capture destruction and admission closure are required. This deliberately is not full upstream multi-item or double-barrier `kthread_q_flush` compatibility. Generation overflow closes admission rather than wrapping. The worker is a primitive only: it does not call RM or implement an NVIDIA ISR/BH.

When an IRQ callback can signal the worker, the owner may use `shutdown_after_irq` to retire that `IrqLine` handle's callbacks before closing admission and draining accepted work. The passed handle must own every producer for this worker. Mask/quiesce the actual device interrupt source first; this software callback-dispatch barrier does not mask hardware, prove cross-CPU APIC/IRTE drain, or stop DMA. Other callback handles/producers require their own retirement before resources are released.

## Tests

Three pure validation tests cover NVIDIA/display identity checks, zero/overflow range validation, and config-read bounds/alignment. All three ran successfully inside OSTD under QEMU TCG via `cargo osdk test`; they do not validate NVIDIA BAR/config I/O against a real GPU. The ordinary CI entrypoint `bash tools/graphics/run_cpu_kernel_tests.sh OUTPUT_DIRECTORY` runs this suite as well as NVport and OSTD without KVM or device passthrough.

## RM integration boundary and GSP gap

Useful NVIDIA open-kernel source/API targets for a later real integration include `nv_pci_probe_body()` / `nv_pci_probe()` and BAR validation/collection (`nv_pci_validate_bars()`, `nv->bars[*].cpu_address`, `NV_PCI_RESOURCE_START()` / `NV_PCI_RESOURCE_SIZE()`), followed by `nv_start_device()` and RM initialization. This inventory supplies only the PCI identity/resource observation that could inform that work; it implements none of those RM calls or a PCI driver's probe lifecycle.

A functioning recent NVIDIA stack also needs RM-private-state/adapter integration and the actual GSP boot/runtime handshake, in addition to MMIO ownership/mapping, interrupts, device-bound DMA/IOMMU and power/device lifecycle. A separate NVport provider now retrieves release-matched firmware files from guest boot storage, but file acquisition is not GSP loading into GPU memory, signature validation, mailbox/queue setup or RM startup. Creating a resource snapshot must never be presented as proof that GSP started or RM initialized.

## Opt-in real runtime service

The `nvport-runtime` feature installs immutable callbacks backed by Asterinas' initialized monotonic clock, RTC-derived realtime and clocksource counter resolution. Installation occurs at the component's `Kthread` stage, after core time initialization, not during early PCI inventory. A boot round trip exercises unchanged NVIDIA time/thread code, confirms a stable live-task identity across a real scheduler yield, and labels RM/GSP as uninitialized. The nominal counter interval is not a promise of absolute clock accuracy; realtime preserves Asterinas' existing RTC/dummy policy.

The kernel selects this runtime only when both `nvidia-nvport-selftest` and `nvidia-pci-diagnostics` are enabled. A diagnostics-only build still requires neither the NVIDIA C checkout nor the time adapter. The native suite explicitly enables the runtime and adds calendar boundary/leap-year/overflow validation to the three inventory tests.

## Minimal real device lifecycle still to port

In the reviewed NVIDIA source, `nv_pci_probe` / `nv_pci_probe_body` validates and claims PCI/BAR resources and creates per-device/RM private state. Probe-time initialization or first open reaches `nv_start_device`, which establishes power, IRQ and task prerequisites before `rm_init_adapter`. GSP startup then traverses `kgspInitRm_IMPL` / `_kgspBootGspRm` into the hardware-specific bootstrap. Last close/removal requires `nv_stop_device`, `rm_shutdown_adapter`, work/interrupt quiescence and DMA/BAR release in a safe order. The implemented OS services are prerequisites for that lifecycle, not replacements for it.

Asterinas already provides `PciDriver`/`register_driver`, `Bar::acquire`, `IoMem`, IRQ, DMA and scheduler facilities; the work is to supply NVIDIA-specific ownership and correct device-bound semantics, not invent stub equivalents. Its PCI driver interface currently lacks a remove callback, which must be addressed or explicitly bounded to non-hot-unplug lifetime before claiming complete teardown.

DRM additionally needs a real `DrmDevice` and per-open RM state mapped to Asterinas' `DrmMinor`/`DrmFile` lifecycle, GPU-backed GEM, sharing/synchronization and display operations. A successful future compute dispatch still would not establish KMS scanout, Wayland rendering/presentation or full GPUI acceptance.

## Reversible pre-RM resources (supersedes inventory-only API status)

Default component initialization remains read-only. The explicit
`device::NvidiaDeviceResources` API now connects measured NVIDIA BAR metadata to actual
`aster-pci`/OSTD ownership. It is deliberately not a `PciDriver`: a successful probe advertises
a working device, which this incomplete RM port cannot supply. There is no substitute RM crate
or fake-success initialization callback.

Obtain `PCI_BUS.lock().reserve_unclaimed(location)` and call
`NvidiaDeviceResources::prepare_reserved(&mut reservation)`. This validates NVIDIA display
identity, non-prefetchable BAR0 and measured ranges, then acquires real uncacheable MMIO through
`PciMmioGuard`. The reservation returns the same unbound function to the queue on drop. A
resource guard cannot outlive its reservation, and neither mutable device access nor a mapping
clone is exposed through that path. A supported NVIDIA chip is NOT established by the identity
check. No GPU register reads or firmware uploads occur automatically; explicit register reads
must first satisfy NVIDIA's actual BAR firewall/power prerequisites.

Preparation rejects pre-existing bus mastering, enabled MSI/MSI-X, non-D0 power states, invalid
capability lists, unavailable mappings, and interrupt-context acquisition. It enables memory
decode while keeping DMA and INTx disabled. Ordinary failures unwind acquired resources and
restore the original command. Final release unmaps/recycles MMIO before command restoration.
Restoration is best-effort on present, responsive hardware; native QEMU tests verify readback.
This is boot-time/non-hotplug resource ownership, not GPU reset, DMA quiescence, ISR/worker
shutdown or hot-unplug. Existing raw PCI configuration APIs remain available to trusted kernel
code; callers must not use them to bypass the reservation's DMA/interrupt restrictions.

Supporting core fixes remove enumeration's unconditional grant of bus mastering, preserve the
firmware command across BAR sizing, and avoid clearing adjacent Status W1C flags during Command
writes. Capability walking is bounded already during enumeration, rejecting cycles and short
MSI-X headers. IoMem clones/slices share one backing reservation; only final-owner teardown
unmaps and recycles the region. Sensitive/system-device reservations never return to the public
allocator. Allocator operations exclude local IRQs to prevent final-drop IRQ reentrancy.

The native CPU test entrypoint now also runs `aster-pci` with QEMU's genuine `pci-testdev`
(`1b36:0005`, not a spoofed NVIDIA identity). Eight reserve/map/register-read/bounds-check/
rollback/reacquire cycles exercise the same generic mechanism used by the NVIDIA adapter.
A separate OSTD test checks clone/slice final-owner lifetime using a synthetic reservation and
empty virtual area. Neither test executes NVIDIA hardware, RM, GSP, GPU DMA or a benchmark.
The existing local Actions workflow invokes the expanded suite; no remote run is implied.

The reviewed official NVIDIA tree remains 615.78.08 at
`1a82fbfda8f321b056eb3568850f9316c5e465c5`. Its `kernel-open/nvidia/nv-pci.c`
`nv_pci_probe_body` additionally needs `rm_wait_for_bar_firewall`,
`rm_is_supported_pci_device`, per-device RM state, and an initial 32-bit DMA mask/max segment
size. CXL ownership and ReBAR reassignment are separate, unimplemented cases. In
`kernel-open/nvidia/nv.c`, `nv_start_device` establishes power, IRQ and worker prerequisites
before `rm_init_adapter`; GSP reaches `kgspInitRm_IMPL`/`_kgspBootGspRm`. Last close/removal
requires genuine `nv_stop_device`/`rm_shutdown_adapter` and ordered task/IRQ/DMA teardown.

## Device-bound DMA: explicit direct mode, not an IOMMU substitute

The new OSTD `DirectDmaContext` allocates actual zeroed, physically contiguous untyped frames
independently of the legacy global `DmaCoherent`/`DmaStream` mapping path. It carries a validated
segment/BDF and an inclusive device address mask. `PciMmioGuard::dma_context` derives identity
from the real enumerated function; `NvidiaDeviceResources::dma_context` uses that same service.
Buffers borrow the context, which borrows the MMIO owner/reservation. There is no clone/split
or physical-address escape through this buffer API. Production acquisition never enables bus
mastering, starts transfers or installs an interrupt handler. NVIDIA's initial mask must be
32 bits until real RM negotiation establishes otherwise.

This mode deliberately has **no hardware isolation**: DMA address equals physical address on
the supported coherent x86 platform. A requester tag alone cannot enforce access control.
IOMMU/CVM configurations and unsupported/unknown platforms fail explicitly, not via a wrapper
that pretends the existing global mappings are device-specific. Every allocation validates its
last byte against the supplied mask, rejects arithmetic overflow/zero size, and frees on mask
failure. There is currently no constrained low-memory allocator, bounce buffering, streaming
pin/map API or maximum-segment/boundary negotiation; an unreachable allocation returns an error.
Completion/device-specific quiescence must precede buffer destruction. Lifetimes keep owners
alive but cannot infer that hardware finished DMA. Hot removal remains unsupported.

The absence check does not use `acpi::AcpiTables::headers()`: that iterator silently omits
invalid SDTs and could mistake corrupt DMAR/IVRS for absence. The new bounded strict root
inventory validates the RSDP/root and every immediate SDT child, including its full checksum,
returning an error instead of skipping malformed entries. Missing ACPI and any such error deny
direct DMA. As in the existing OSTD ACPI backend, firmware pointers must still refer to readable,
immutable, linearly mapped firmware memory; numerical bounds are not proof that arbitrary
corrupt physical pointers are mapped. This is a trusted-firmware boundary, not hostile-firmware
containment, and only the currently supported TDX runtime detects confidential guests.

The PCI test suite uses the genuine [QEMU EDU DMA controller](https://www.qemu.org/docs/master/specs/edu.html)
(`1234:11e8`, configured with a 32-bit hardware mask), not a mocked callback or NVIDIA identity.
It programs actual RAM-to-device and device-to-RAM transfers with poisoned destination pages,
checks every byte, disables bus mastering before releasing completed buffers, and repeats
reserve/allocate/transfer/release. A timeout quarantines both allocations instead of freeing
potentially active DMA memory. Register writes and BME enabling exist only in the EDU test;
the production pre-RM adapter still exposes neither. A second boot with QEMU Intel IOMMU must
reject direct DMA without issuing a transfer. The ordinary local/Actions CPU entrypoint requires
both the actual transfer marker and the separate IOMMU rejection marker.

That second boot exposed an existing VT-d MMIO fault: register construction used
`paddr_to_vaddr(0xfed90000)` without mapping the MMIO aperture. The backend now owns a sensitive,
uncacheable `IoMem` mapping for its register views instead of assuming a linear RAM alias.
Capability-selected fault-record and IOTLB windows must fit that measured one-page mapping
before their pointers are constructed; larger layouts are explicitly unsupported. Fixing this
register access does not supply isolated translation domains or completed IOTLB invalidation.

### Exact IOMMU blockers, before safe RM/GSP initialization

The legacy x86 implementation cannot yet supply a device-isolated domain:

1. `ostd/src/arch/x86/iommu/dma_remapping/mod.rs` maps via BDF zero and initializes every BDF
   with a shallow copy of one shared translation root; its identity lacks PCI segment.
2. `dma_remapping/context_table.rs` installs entries without assigning distinct Domain IDs.
   `registers/mod.rs` selects one DRHD rather than resolving segment/device scopes across units.
3. DMAR RMRR structures are parsed but their required mappings and requester associations are
   not enforced. PCI bridge requester aliases/ACS groups and ATS/device-TLB handling also need
   explicit policy; a BDF-only domain must not be advertised as isolation around a bridge.
4. `ostd/src/mm/dma/util.rs::unmap_dma_remap` explicitly lacks completed IOTLB invalidation.
   Context/IOTLB/device-TLB invalidations, completion ordering and domain/IOVA reclamation must
   precede freeing frames. Existing queued invalidation for interrupt entries is not this work.

The safe next remapping implementation needs resolved DRHD/scopes, owned domain IDs and page
tables, mask-constrained IOVA allocation, transactional page mappings, completed invalidation
and device-specific stop/reset before memory/domain reuse. Enabling NVIDIA DMA on the present
shared/unrevoked translations would not meet that contract. Direct-mode tests do not validate
IOMMU map/unmap and must never be described as doing so.

Full RM/GSP initialization still needs supported-chip/BAR-firewall checks, private adapter
state, real power/IRQ/worker lifetimes and a reviewed foreign-code/unsafe boundary before
`nv_start_device`/`rm_init_adapter`. There is no authenticated matching firmware here and no
NVIDIA GPU to perform the real bootstrap/upload/handshake or ordered shutdown. No fake probe
success or GSP callback is supplied to bypass those missing contracts. Full display additionally
requires the RM ioctl/mmap ABI, GPU memory/submission/fences/reset, GPU-backed GEM, sharing and
syncobj/timeline/atomic KMS integration, then native Vulkan/Wayland/GPUI presentation. A Linux
host compute pass or an Asterinas EDU transfer supplies none of those hardware acceptances.
Previously extracted firmware remains unverified CPU-test-only data and is not loaded/executed
by this path. Full NVIDIA driver and GPU benchmark acceptance remain unmet.

## Atualização: domínios VT-d limitados, sem inicialização NVIDIA

`NvidiaDeviceResources::isolated_dma_context` encaminha a posse PCI/MMIO ao novo caminho
de tabelas privadas. Não habilita bus mastering, não chama RM/GSP e não recorre ao mapper
compartilhado quando uma validação falha. Os bloqueios anteriores continuam válidos para
o port completo, mas o subconjunto abaixo já possui implementação separada do mapper legado.

O backend exige um único DRHD Intel no segmento zero e associação por escopo DMAR explícito.
Escopos de endpoint correspondem ao caminho completo ou ao BDF/barramento final; escopos de
bridge enraizados no bus zero autorizam apenas descendentes estritos. `INCLUDE_PCI_ALL` não
concede autorização implícita. Havendo MCFG, a política exige uma única região válida no
segmento zero, iniciando no bus zero, sem tabelas duplicadas. RMRR, ATSR e múltiplos DRHDs
continuam recusados.

PCIe agora tem um subconjunto conservador: endpoint tipo zero, devfn 00.0, função única,
imediatamente abaixo de um root port PCIe verdadeiro, rota única com secondary=subordinate,
sem peers adicionais. ACS Source Validation, P2P Request/Completion Redirect e Upstream
Forwarding precisam existir e estar ativos; Direct Translated P2P ativo é recusado.
ATS/PRI/PASID, ARI/SR-IOV, funções múltiplas, switches, bridges legadas e rotas ambíguas são
recusados antes de BME. Translation Blocking opcional não é exigido: serviços de tradução do
endpoint são recusados e VT-d TT=00 rejeita pedidos traduzidos. A produção só lê configuração;
o driver/firmware confiável não pode mudar a rota/ACS durante a vida do domínio, e hotplug não
é suportado. A associação não transforma BAR0 em região segura antes do firewall/power NVIDIA.
**Isso não é suporte NVIDIA**: adapters multifunção comuns permanecem fora do subconjunto,
e supported-chip/firewall, IRQ/workers, o port RM e a inicialização GSP ainda faltam.

Cada domínio tem DID próprio (1–15; zero permanece com o mapper legado), raiz e páginas
de tradução próprias. IOVAs obedecem à máscara inclusiva e não são reutilizados durante a
vida do domínio. A revogação aguarda context-cache, IOTLB, drenagem de leituras/escritas e
Wait/Fence reais. Páginas intermediárias só são liberadas após detach concluído. Timeout
envenena o backend e mantém memória/tabelas/DID em quarentena. Isso não transforma os
outros drivers/dispositivos que ainda usam o mapper legado em domínios isolados.

O primitivo OSTD mapeia apenas armazenamento não tipado que possui; receber um BDF não
autentica um dispositivo. O wrapper PCI associa o BDF ao dispositivo enumerado e mantém os
empréstimos de MMIO/reserva. A conclusão legítima de operações continua sendo obrigação do
driver. `IsolatedDmaBuffer::revoke` permite reter RAM para CPU após remover o IOVA antigo.

O runner local/Actions exige transferências EDU reais com comparação integral, separação
entre dois requesters com IOVAs iguais, escritas negadas após revogação com a RAM ainda
possuída, rejeição de contexto duplicado/máscara impossível e detach/reaquisição. Também exige
um controlador NVMe PCIe verdadeiro atrás de root port real: filas SQ/CQ privadas e Identify
por DMA, CID/status/vendor/serial/model conferidos, controller parado antes de liberar memória.
Casos negativos cobrem ACS ausente/desligado e EDU atrás de bridge legada. Escritas ACS e BME
do NVMe existem apenas no teste QEMU, não no caminho de produção. EDU/NVMe/QEMU não são NVIDIA:
nenhum desses marcadores é benchmark GPU, inicialização RM/GSP ou aceitação Vulkan/Wayland/GPUI.
O workflow permanece preparação local, sem execução remota implícita.

## Worker nativo com posse e drenagem de callback

`worker::DeviceWorker` cria uma tarefa OSTD real para um callback `FnMut + Send + 'static`,
com proprietário exclusivo e sinais `WorkSignal` clonáveis. `schedule` pode sinalizar de
tarefa ou IRQ mascarável: não cria um work item por solicitação nem executa o callback
inline. O estado usa `SpinLock<_, LocalIrqDisabled>`; o wake do OSTD ocorre fora desse lock.
Não é NMI-safe. Solicitações repetidas coalescem enquanto pendentes; uma solicitação durante
execução produz uma nova invocação pendente. O callback deve drenar o serviço real do device.

`shutdown` exige tarefa sleepable, recusa autoespera, fecha admissão sob o mesmo lock usado
por `schedule`, drena a pendência aceita e só publica completion após destruir o callback
e seus captures. O callback pode reagendar a si mesmo enquanto a admissão estiver aberta.
Depois do fechamento, sinais sobreviventes retornam `Closed`; não mantêm o serviço aberto.
Drop apenas fecha e acorda, sem bloquear ou destruir captures em IRQ: a tarefa continua
possuindo-os até terminar a drenagem. Antes de liberar estado externo do device, o chamador
deve executar shutdown explícito sem segurar locks usados pelo callback ou seus destrutores.
Um callback que nunca retorna não tem drenagem finita garantida. Completion não é join da
tarefa do scheduler, parada DMA, IRQ/MSI drain, reset ou sucesso RM/GSP.

O contrato de coalescência/reagendamento segue o item BH pré-inicializado descrito em
`kernel-open/common/inc/nv-kthread-q.h` da fonte oficial NVIDIA 615.78.08. Este serviço é
um callback único, **não** um port completo da fila FIFO multi-item, de `os_queue_work_item`
ou dos workers RM. `rm_isr`, `rm_isr_bh_unlocked` e `rm_init_adapter` não são chamados.
A posse PCI/MMIO atual continua emprestada e não pode ser capturada como se fosse `'static`;
a transferência real para um owner de adapter e os stacks/ABI RM permanecem pendentes.

Seis testes adicionais rodam no kernel OSTD: execução em outra tarefa, coalescência e
drenagem com destrutores, rejeição de contextos atômicos/autoespera, Drop não bloqueante,
reagendamento dentro do callback e handoff de uma **interrupção APIC timer real** para tarefa
sleepable. O teste timer deixa somente um callback inerte com slot vazio porque OSTD ainda
não expõe unregister para esse timer. Não imita IRQ NVIDIA. O runner usado pelo workflow
local exige os marcadores de IRQ real e drenagem, sem convertê-los em aceitação GPU.
