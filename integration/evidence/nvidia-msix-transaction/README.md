# PCI MSI-X transacional — checkpoint local validado

**Não é um driver NVIDIA completo. Não há aceitação GPU.** Fonte revisada: [NVIDIA 615.78.08, Latest oficial](https://github.com/NVIDIA/open-gpu-kernel-modules/releases). RM/GSP, ABI/submissão e Vulkan/Wayland/GPUI acelerados continuam incompletos. Sem `/dev/nvidia*`, `/dev/dri` ou `/dev/kvm`; nenhum firmware GPU carregado.

## Entrega

Depois dos treze predecessores listados em `validate_stack.py`, aplicar:

1. `/workspace/asterinas-gpui/local-prs/asterinas-nvidia-msix-transaction.patch`: aquisição mascarada, handlers antes de unmask, parada/sincronização/rollback/quarentena; consumidores NVMe/Virtio; posse PIO limitada real.
2. `/workspace/asterinas-gpui/local-prs/asterinas-irq-retire-storage.patch`: libera armazenamento dos callbacks, sem reciclar IRQ aposentado.

Quinze patches aplicam limpos à base: 96/97 arquivos idênticos, única exceção conhecida de ordem de imports DRM; zero divergências inesperadas. Índice Git não mudou. Sem commit/push/autenticação/Actions remoto ou alteração do agendamento.

## Resultados reais finais

- Kernel: **20 NVport, 4 plataforma, 13 PCI, 13 PCI+VT-d, 256 OSTD** reportados; zero falhas. Três corpos VMX/VMCS/EPT explicitamente skipped.
- **38 testes host** passaram.
- NVMe MSI-X: Identify/CQ e Table/PBA reais, callback instalado antes de DMA e silencioso enquanto mascarado, entrega após unmask, dois ciclos de parada com RDY=0/BME-off. Exaustão total/parcial, rollback, erro/retry, destruição de capture, contexto atômico e abandono/quarentena também testados. Identidades QEMU não alteradas.
- Kernel completo: **NVMe + Virtio moderno + legado** leram padrões, escreveram, fizeram fsync/readback. Os três discos raw também contêm os bytes corretos examinados no host. Guest desligou com código 33.
- Três boots completos da mesma ISO — normal, VT-d e multifunção VT-d — chegaram ao `/init` e poweroff 33.
- Actionlint do workflow gráfico, diff check e round-trip dos patches passaram.

## Evidências e reprodução

`run_final_validation.py` executa sequencialmente dez fases para evitar builds concorrentes; `final-validation-phases.json` registra exit 0 em todas.

- `test-counts.json`, `native-suites-storage-final/`: contagens, skips, hashes e logs reais do kernel.
- `regression-results.json`, `old-implicit-bme.log`, `old-index-panic.log`, `fixed-legacy.log`: regressões falham no guest, corrigida passa; fonte restaurada.
- `consumer-io/run-final/result.json`, `serial.log` e três `.raw`: I/O e conferência host. Fontes/reprodutor: `consumer-io/init.c`, `run_consumer_io.py`.
- `cpu-boot-final/result-plain.json`, `result-iommu.json`, `result-multifunction-iommu.json`: boots/hashes; `run_cpu_boot.py` reproduz.
- `patch-validation.json`, `full-stack-validation.json`: round-trip e ordem conjunta.
- `source-msix-final/`: snapshot imutável do patch 14; `make_patch.py` usa esse snapshot para não absorver o patch 15.
- `upstream-sources.md`: contratos e próxima fronteira RM.
- `nvidia-benchmark-result.json`: rejeição estrita por ausência de GPU, nenhum JSON de passagem.

As tentativas `consumer-io/run/` (TODO PIO legado, panic) e `run-pio-v2/` (I/O real concluído, mas marcadores intercalados recusados pelo parser) foram preservadas como falhas. A emissão atômica corrigida passou em `run-final/`, sem remover exigências. Rodadas intermediárias `native-suites/` e `native-suites-final/` não substituem os logs finais `native-suites-storage-final/`.

## Segurança e próximos passos

Vetores ativados ficam reservados como sinks até reboot: ainda não existe prova de drain APIC/IRTE/SMP para reciclagem segura. O driver deve realmente parar hardware/DMA e drenar workers; não se infere isso de máscaras ou de uma closure de sucesso. Drop ativo sem parada confirmada conserva recursos/quarentena. A política é fail-closed e finita, não recuperação GPU madura.

A próxima ponte exige admissão/firewall/suporte RM, estado/power/stacks/IRQ/workers e callbacks DMA reais, seguidos de firmware autenticado e GSP. Firmware de instalador truncado continua não verificado e nunca deve ser carregado numa GPU. CPU/QEMU não substitui benchmark GPU físico sob Asterinas.

Após a validação, o disco ficou cheio ao salvar as notas; apenas caches Rust `target/.../debug/incremental` regeneráveis foram removidos. Todos os códigos, patches, logs, ISOs e discos de evidência foram preservados.
