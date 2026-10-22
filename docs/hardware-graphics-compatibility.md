# Gráficos e hardware: o que já foi provado

**A ISO já instalou em UEFI e o sistema instalado chegou ao stage-2 do Asterinas, mas não ao login.** No run `39715117705` da base `cecd1ce`, instalação e verificação EFI GRUB passaram; o boot instalado carregou kernel/initrd, iniciou `/init` e parou no último ponto observado `running activation script...`, até o timeout de login. **Não passou hello, desktop/GPUI nem aceite completo da ISO.** Foi acrescentado tracing opt-in da ativação para tornar a próxima falha localizável, sem mudar os gates.

**Os FileOps reais de fbdev passaram oito casos em QEMU**, sobre uma superfície sintética de 24 bytes no MMIO VGA real: transferência curta, EOF/ENOSPC, offset extremo e cursor de leitura/escrita vazio. Os metadados informam `smem_len=24` e pitch 12; oito bytes de guarda permaneceram intactos. **Não é uma prova de syscall/ioctl/mmap, GPUI ou módulo Linux executado.**

**Etapa anterior:** o console passou a dimensionar seu buffer e seu scroll pelo span lógico, corrigindo dois panics reproduzidos em QEMU. O framebuffer mantém as escritas limitadas ao span e a prova de readback. A medição real continua em **1280 × 800, 32 bpp, pitch de 5.120 bytes e superfície de 4.096.000 bytes**. Isso não demonstra GPUI nativo, aceleração gráfica ou imagem apresentada em monitor físico.

## ISO instalada: boot real, bloqueio na ativação

O [run da base `cecd1ce`](https://github.com/cybermanju/cybermanju-shell/actions/runs/39715117705) terminou com falha no boot instalado. Os passos anteriores concluíram o live USB e a instalação da ISO completa em UEFI/TCG, incluindo o marcador final, checagem das partições e arquivos EFI GRUB e desligamento. No boot seguinte, o EFI stub carregou cmdline/initrd, detectou framebuffer 1280×800, decodificou o ELF, e o kernel iniciou a primeira thread, desempacotou initramfs e executou `/init`. O console mostrou `<<< Asterinas NixOS Stage 2 >>>`, a configuração do sistema e `running activation script...`.

A falha foi **timeout de 2.400 segundos esperando `root@asterinas:`**, não evidência de prompt ou hello. O último comando de ativação não era visível. Não foi identificado qual syscall, script ou espera bloqueou; silêncio não prova panic nem apenas lentidão TCG. Esse resultado supera o antigo bloqueio de conclusão da instalação, mas não valida login, userspace hello, apresentação gráfica ou hardware físico. Logs integrais em `artifacts/cycle-0330/iso-failed.log`; console separado em `installed-console-excerpt.log` e saída guest decodificada em `installed-guest-output.log` no mesmo diretório.

A nova opção **`aster_nixos.trace-activation` é booleana e desligada por padrão**. Só o guest descartável de CI recebe uma configuração embrulhada que a habilita: o módulo original é incorporado como expressão, com imports relativos preservados ao ser copiado para `/etc/nixos`, sem depender de um arquivo `/root` externo. A preparação não sobrescreve destinos existentes e o parser Nix deve aceitá-la antes de iniciar a instalação. A ISO publicada continua interativa e o fluxo normal sem tracing.

O [helper](../asterinas/distro/etc_nixos/modules/trace-activation.sh) insere PS4 com linha e `set -x` depois do shebang de `activate`, preservando intérprete/opções, corpo, modo e status de saída. Se houver symlink, substitui só o arquivo de saída, sem editar o alvo compartilhado. Scripts ausentes, não Bash ou com sintaxe inválida são recusados, com limpeza do temporário. O hook é `mkAfter`: o pin Nixpkgs `fd1462031fdee08f65fd0b4c6b64e22239a77870` cria `activate` em `activatableSystemBuilderCommands`, antes da instrumentação. Os comandos existentes do core mantêm a prioridade anterior.

**Xtrace pode expor argumentos e segredos: não habilitar em configurações pessoais ou com credenciais.** Os logs `CYBERMANJU_ACTIVATION` são somente diagnóstico, nunca marcadores de aprovação. Continuam obrigatórios login, `Hello Asterinas!`, desligamento e status de sucesso; deadlines e capacidade de disco não foram alterados. Nenhum gate passou a aceitar stage-2 ou tracing como sucesso.

Validação local: **192 testes Python**, incluindo oito testes novos que executam helper/wrapper e preservação de falhas; parsing real com Nix 2.18.1 dos dois módulos modificados e do wrapper da configuração real; `bash -n` e diff. As provas de shell não são um boot NixOS. O parser não avalia o módulo completo nem constrói um closure; o novo hook e tracing **ainda não foram validados no boot da ISO**. Passou também o ktest preparado de módulos inalterado como regressão em QEMU, sem executar init/exit. Evidências em `artifacts/cycle-0330/` e `artifacts/staging-0330/`. A ISO da base `5dd5209` ainda estava na etapa de instalação na última consulta; ela não contém esta nova instrumentação.

## fbdev: não confundir bytes de imagem com capacidade mapeada

O [consumidor fbdev](../asterinas/kernel/core/src/device/fb.rs) usava `IoMem::size()` para leitura, escrita e `smem_len`. Esse valor inclui a cauda até a próxima página, embora `FrameBuffer::byte_len()` delimite a superfície com seu padding de linha. No modo VGA observado os tamanhos coincidem; a diferença foi reproduzida com uma superfície curta sintética sobre a abertura VGA real, não com uma nova resolução de firmware.

O [framebuffer](../asterinas/kernel/core/comps/framebuffer/src/framebuffer.rs) agora fornece `surface_io_mem()`: um slice `0..byte_len` que compartilha a mesma reserva/mapping por `Arc`, sem nova aquisição ou alteração de cache. Somente `read_at` e `write_at` de fbdev passam por essa view; `smem_len` informa o span lógico. O código mantém EOF de leitura, ENOSPC para escrita não vazia a partir do fim e operações truncadas ao tamanho restante. A prova histórica abaixo cobre a camada MMIO; o novo teste direto de FileOps descrito a seguir exercita também o consumidor, mas não syscalls. Conversões preexistentes para campos `u32`, inclusive superfícies acima de 4 GiB, não foram qualificadas.

**Prova antes/depois:** substituir a nova view por um clone bruto fez o ktest falhar exigindo **24 bytes**, quando a capacidade observada era **4.096.000**. O fonte correto foi restaurado. Depois passaram quatro casos: leitura e escrita fallible de dois bytes a partir do offset 23 foram recusadas antes do I/O, sem consumir cursor, alterar destino ou escrever prefixo; escrita e leitura de um byte no offset 23 passaram. Os oito bytes de guarda `[24,32)` permaneceram intactos e os bytes originais foram restaurados. A view também preservou a origem física. Essa prova não mede uma nova característica de GPU.

O [gate](../tools/ci/verify_framebuffer_runtime.py) exige agora cinco marcadores de hardware completos, incluindo `surface_view:PASS cases=4 bytes=24 guard_bytes=8`. Rejeita mistura desse marcador no modo contract e mantém os testes anteriores de geometria/console sem mascará-los por marcador ausente. O JSON registra `surface_view_verified=true`, mas **`fbdev_userspace_verified=false` e `mmap_tail_isolated=false`**. O caminho bruto `io_mem()` e o mmap page-granular permanecem; não prometemos isolamento contra aliases nem contra acesso à cauda por mmap.

Validação desta etapa: **169 regressões Python**, os dois ktests gráficos em VMs separadas e type-check de `aster-core`/`aster-framebuffer` para `x86_64-unknown-none`; rustfmt, sintaxe e diff. Evidências em `artifacts/framebuffer-view-runtime/`, `artifacts/framebuffer-contract-2200/`, controle negativo em `artifacts/framebuffer-view-negative/` e logs em `artifacts/cycle-2200/`. Também passaram staging bruto e fixture preparada em VMs separadas: **quatro ktests positivos no total**, sem chamar init/exit. Evidências adicionais em `artifacts/staging-2200/` e `artifacts/prepared-2200/`. A compilação de `aster-core` verifica o consumidor real, não boot ou syscall. O primeiro comando direto parou por falta de `VDSO_LIBRARY_DIR`; a repetição passou com o ELF64 x86-64 real de `asterinas/linux_vdso` no pin `8af5717` do [Dockerfile oficial](../asterinas/tools/dev_env/docker/kernel-dev/Dockerfile), sem stub. Artefato, commit completo e metadados estão em `artifacts/cycle-2200/linux-vdso/`; o log aprovado é `cargo-check-core-final.log`. Para reproduzir dentro de `asterinas`, fornecer `VDSO_LIBRARY_DIR=<diretório-validado>` ao `cargo check --locked -p aster-core -p aster-framebuffer --target x86_64-unknown-none`. O teste direto de FileOps descrito a seguir avança a integração com superfícies não múltiplas de página; syscall/mmap, endereço físico não alinhado e cores completas continuam pendentes.

## FileOps reais: truncamento, EOF e cursores

O [ktest do consumidor](../asterinas/kernel/core/src/device/fb_runtime_tests.rs) liga `aster-core` e chama `FbHandle::read_at`/`write_at` diretamente, usando `VmReader`/`VmWriter` fallible apoiados em buffers do kernel. São **quatro casos de leitura e quatro de escrita dentro de um único ktest**: cruzamento do fim a partir do byte 23, offset 24, offset `usize::MAX` e cursor vazio nesse offset extremo. Confere o tamanho e avanço dos cursores, conteúdo lido, byte escrito, guarda `[24,32)` e restauração. Também chama `collect_fix_screen_info` diretamente; não despacha o ioctl.

A superfície 2×2, pitch 12, é sintética sobre o mapeamento VGA de boot, não uma resolução nova de firmware. O helper existe apenas sob `cfg(ktest)` e inicializa o framebuffer de boot se necessário. A VM dedicada é descartável, com uma CPU; o teste não registra fbdev nem inicia o kernel de produção. O marcador `fbdev_init_called=false` refere-se ao inicializador de registro de fbdev, **não** à ausência de inicialização do framebuffer.

O [harness específico](../tools/ci/fbdev-runtime.OSDK.toml) passa `earlycon` via `kcmd_args`, antes de `--`: o parser real de `aster-cmdline`, ao contrário do fallback OSTD, deixa o serial desabilitado sem esse token. O primeiro boot terminou com exit de sucesso, mas sem serial; o gate o recusou por ausência de resumo e marcadores. Nenhuma aprovação se baseia apenas no exit do QEMU. O [verificador](../tools/ci/verify_fbdev_runtime.py) exige os três marcadores exatos e um único resumo de 1 passed/0 failed, rejeitando duplicatas, falhas contraditórias, corrupção e CR isolado.

O [preparo do VDSO](../tools/ci/prepare_kernel_vdso.sh) baixa o ELF64 x86-64 ET_DYN real no commit `8af571775a3d3a1df3e84fc9709b9d6b160c771a`, prefixo usado pelo Dockerfile oficial. Exige SHA-256 `27cd7bf576a92fa48a891c05988f52fac35b7374d5b9650d6b035e477c5d93a3`, HTTPS e validação de formato; preserva arquivos divergentes e recusa symlinks. Não usa stub. O runner registra hashes da biblioteca, helper, harness, imagem e verificador. O JSON continua com `userspace_syscall_verified=false`, `mmap_tail_isolated=false` e `linux_modules_executed=false`.

**Controle negativo observado:** recolocar somente `io_mem().clone()` nos dois FileOps fez o teste falhar na leitura que cruza o fim: retornou **2**, quando o limite lógico exige **1**. O fonte correto foi restaurado e passou novamente. As VMs negativas são destruídas; restauração dos bytes é verificada no caminho positivo.

Validação final: **177 regressões Python**, **3 ktests positivos em VMs separadas** (contrato de boot, framebuffer/console e consumidor fbdev), type-check de produção de `aster-core`/`aster-framebuffer`, rustfmt e sintaxe/diff. Evidências em `artifacts/fbdev-runtime/`, `artifacts/framebuffer-contract-2300/`, `artifacts/framebuffer-hardware-2300/`; negativo em `artifacts/fbdev-negative/` e logs de diagnóstico em `artifacts/cycle-2300/`. A primeira tentativa de compilação parou por imports ambíguos de `println`; a correção usa o macro OSTD explicitamente. O novo gate é acrescentado ao job gráfico existente, sem retirar provas anteriores. Aprovação local não implica CI hospedado nem aceite da ISO instaladora.

## Contrato de mmap: bytes limitados não significam páginas isoladas

A política desta revisão é **preservar o mmap paginado existente, sem prometer isolamento da cauda**. `read_at`/`write_at` e `smem_len` usam o span lógico; `mappable`/`map` continuam oferecendo o `IoMem` bruto. Se a última página contiver bytes posteriores à superfície, esses bytes podem continuar acessíveis pelo mapping. Não se deve tratar `smem_len` como barreira de proteção nem qualificar esse caminho para dados sensíveis ou buffers compartilhados com outro proprietário.

Não foi adicionada recusa de mmap para superfícies curtas: fazê-lo agora mudaria a ABI sem teste do consumidor. Antes de qualificar mmap/GPUI, é necessário escolher e provar uma política: exigir uma superfície inteira em páginas sob posse exclusiva, recusar a página parcial, ou usar um buffer intermediário isolado com cópia/apresentação. Um slice byte-granular sozinho não remove acesso aos demais bytes da mesma página. A validação futura precisa cobrir offsets, comprimento e última página via VMAR/syscall, além de endereços físicos não alinhados. O teste direto de FileOps não substitui essa prova.

## Console: usar o tamanho da superfície, não da página

O [console](../asterinas/kernel/core/comps/framebuffer/src/console.rs) dimensionava seu espelho de pixels pelo tamanho do mapping MMIO arredondado, embora `FrameBuffer::write_bytes_at` valide o span lógico. Quando esses tamanhos diferem, ativar o console ou voltar do modo gráfico tenta um flush grande demais. O scroll também usava a capacidade MMIO ao calcular onde zerar o buffer.

A correção aloca pelo `byte_len()` da superfície e calcula a cauda do scroll pelo próprio buffer. Não altera `IoMem`, o mapping físico ou mmap do dispositivo.

**Dois controles negativos em QEMU:** o teste inicial, com superfície sintética de 24 bytes sobre MMIO real maior, fez o console antigo panicar ao ativar (`InvalidArgs`). Mantendo a alocação corrigida e recolocando somente a conta antiga do scroll, o teste final falhou com índice `4095712` fora de um buffer de 576 bytes. Os fontes corretos foram restaurados antes da validação positiva.

O teste final usa uma superfície **sintética 8×16, pitch 36, span 576 bytes**, com quatro bytes de padding por linha, sobre a abertura VGA real. Exercita as APIs públicas de **ativação, volta Graphics→Text e scroll por duas quebras de linha**. Em cada caminho, lê de volta todos os 576 bytes e confirma que oito bytes sentinela posteriores continuam intactos; restaura os bytes originais ao terminar. Não testa desenho de glifos, toda a máquina ANSI ou novas resoluções reais.

Passaram **2 ktests em VMs separadas** (contrato e hardware/console), **131 regressões Python**, type-check normal de OSTD/ponte/framebuffer, rustfmt e sintaxe/diff. O gate de hardware agora exige também o marcador único `console_flush:PASS cases=3 bytes=576 guard_bytes=8`; logs antigos sem essa prova não aprovam esta revisão. Os seis limites de escrita anteriores continuam obrigatórios. Serial, JSON e hashes em `artifacts/framebuffer-console-fixed/` e `artifacts/framebuffer-contract-1916/`; negativos em `artifacts/framebuffer-console-before/`, `artifacts/framebuffer-scroll-before/` e `artifacts/cycle-1916/`.

## Limites de escrita com prova antes e depois

`write_bytes_at` valida `offset + tamanho` com aritmética verificada e recusa qualquer intervalo além de `byte_len` **antes do I/O**. `write_pixel_at` passa pela mesma validação. Uma escrita vazia é aceita exatamente no fim, mas não depois dele. O span inclui padding de linha; a cauda do arredondamento de páginas não pertence à superfície.

O [ktest gráfico](../asterinas/kernel/core/comps/framebuffer/src/runtime_tests.rs) cria uma superfície **sintética de 24 bytes sobre o mapeamento MMIO real**, que continua maior e acessível. Isso é importante: o modo real observado ocupa páginas completas, então testar apenas seu limite poderia esconder a falta de validação própria no framebuffer. São seis verificações no mesmo ktest: escrita cruzando o fim, escrita começando no fim, pixel fora do span, overflow, escrita vazia depois do fim e escrita vazia exatamente no fim. Uma leitura posterior confere que os bytes em torno do limite permaneceram intactos, inclusive o prefixo que ainda caberia na superfície.

**Controle negativo observado:** sem a correção, o novo teste falhou na primeira escrita cruzando o limite. Com a correção, passou junto com o readback real e a restauração dos pixels. Não são seis ktests independentes nem uma medição de padding fornecido pela GPU.

O acesso de baixo nível `io_mem()` continua exposto a código confiável do kernel e cobre o mapeamento por páginas. Esta correção protege as duas APIs de escrita do `FrameBuffer`; **não é uma fronteira de isolamento de hardware nem impede acessos por aliases**.

## Contrato de boot preservado

O [descritor de boot](../asterinas/ostd/src/boot/mod.rs) preserva o pitch do bootloader nas conversões de Linux boot, Multiboot v1 e Multiboot2. O span reservado é `pitch × height`, incluindo padding; dimensões nulas, pitch insuficiente, formatos fora da lista admitida e overflow são recusados antes de publicar metadados.

O [consumidor gráfico](../asterinas/kernel/core/comps/framebuffer/src/framebuffer.rs) usa esse pitch, valida o arredondamento do mapeamento e trata indisponibilidade do MMIO sem panic. O clear usa blocos de até uma página, sem alocar outra superfície inteira nem limpar a cauda de arredondamento.

O [header Multiboot2](../asterinas/ostd/src/arch/x86/boot/multiboot2/header.S) solicita framebuffer **opcional**, com preferência por 32 bits e geometria escolhida pelo loader. Sem a solicitação, o GRUB podia entregar apenas EGA/texto, que o parser Multiboot2 rejeita como framebuffer de pixels. Os testes usam o GRUB padrão, sem wrapper, fonte ou terminal gráfico customizado.

## Matriz de evidências

| Caminho | Resultado observado | Limite |
|---|---|---|
| Arquitetura e ferramentas locais | x86-64/q35, TCG, 1 CPU, 1 GiB, BIOS/GRUB Multiboot2; QEMU `8.2.2`; Rust `1.99.0-nightly`, pin `nightly-2026-07-21` | Não qualifica outra arquitetura, firmware ou GPU física |
| Contrato de boot | 1 ktest aprovado: padding sintético, span da região, overflow e conversão de tags Multiboot2 | Sem I/O de GPU |
| Framebuffer VGA | 1 ktest aprovado: clear amostrado, escrita/leitura/restauração de dois pixels na primeira e última linha, mais seis verificações de limites | Readback de MMIO; limites extras usam superfície sintética, não scanout |
| View lógica para fbdev | 4 casos dentro do ktest gráfico; superfície sintética de 24 bytes, 8 bytes de guarda e origem física preservada | Não exercita syscall, truncamento dos FileOps em runtime ou isolamento do page-tail de mmap |
| FileOps fbdev diretos | 1 ktest aprovado em x86-64/q35/TCG, 1 CPU, 1 GiB, GRUB/Multiboot2; oito casos e metadados | Buffers do kernel; sem syscall/ioctl/mmap/registro fbdev |
| Nomes de partições do instalador | Regra shell real executada isoladamente; comparação antes/depois de NVMe/eMMC/NBD e preservação de loop/virtio | Somente strings; sem I/O de disco, controlador, instalação ou boot desses dispositivos |
| ISO completa UEFI, base `cecd1ce` | Live USB + instalação + EFI GRUB passaram; kernel instalado e `/init` chegaram ao stage-2 | Login expirou em ativação; hello/aceite completo falharam; somente QEMU/TCG, sem GPUI/hardware físico |
| Adaptadores de produção | `cargo check --locked -p ostd -p linux-module-staging -p aster-framebuffer --target x86_64-unknown-none` aprovado | Type-check, não boot do kernel completo |
| Regressões históricas da etapa de limites | 109 testes Python, 115 testes ELF e 15 testes host de staging aprovados | Não executam init/exit de `.ko` |
| CI da base `34dd887` | [Sete jobs concluídos com sucesso](https://github.com/cybermanju/cybermanju-shell/actions/runs/39701305916), confirmados pelo endpoint de jobs | Não é aprovação automática da nova view; ISO `39701305949` seguia em execução na consulta inicial |
| CI anterior `4133000` | [Sete jobs de regressão aprovados](https://github.com/cybermanju/cybermanju-shell/actions/runs/39656869405), incluindo framebuffer e staging real | Evidência daquela revisão, não aprovação automática desta mudança |

As VMs dos ktests gráficos são imagens mínimas OSTD, **não a ISO instaladora nem a instalação UEFI completa**; a linha de ISO acima é uma evidência separada. Nenhum módulo, versão Linux ou arquitetura adicional foi qualificado nesta mudança; a [matriz de módulos](linux-module-compatibility.md) mantém as fixtures e suas restrições.

Reprodução a partir da raiz:

```sh
bash tools/ci/test_framebuffer_runtime.sh artifacts/framebuffer-contract --kind contract
bash tools/ci/test_framebuffer_runtime.sh artifacts/framebuffer-hardware --kind hardware
bash tools/ci/prepare_kernel_vdso.sh artifacts/kernel-vdso
VDSO_LIBRARY_DIR="$PWD/artifacts/kernel-vdso" bash tools/ci/test_framebuffer_runtime.sh artifacts/fbdev-runtime --kind fbdev
```

O [runner](../tools/ci/test_framebuffer_runtime.sh) recusa configurações preexistentes, invalida sucesso antigo, limita a VM a 600 segundos e reconstrói só o bundle descartável. Os diretórios guardam serial, JSON verificado, GRUB extraído da imagem, manifests/lockfile, versões e hashes. O [verificador](../tools/ci/verify_framebuffer_runtime.py) agora exige também o marcador único dos seis limites; serial antigo sem essa prova não aprova a nova versão. Panics contraditórios também são recusados.

## Infraestrutura gráfica: correção confirmada no CI

O [CI de `ec4b7f2`, run 39681738837](https://github.com/cybermanju/cybermanju-shell/actions/runs/39681738837), aprovou seis dos sete jobs, mas o gráfico parou ao atualizar APT: `packages.microsoft.com` retornou HTTP 403. As etapas de contrato e MMIO foram puladas; isso não é uma regressão gráfica demonstrada. O log bruto está em `artifacts/cycle-1900/ci-failed.log`. Não se deve aprovar o gate ignorando o erro ou dispensando assinaturas.

O [CI posterior de `9142a04`, run 39684369567](https://github.com/cybermanju/cybermanju-shell/actions/runs/39684369567), **passou nos sete jobs**, inclusive as duas VMs gráficas. Isso confirma a correção de setup naquela revisão, não aprova automaticamente o novo código de console.

O novo [helper de instalação](../tools/ci/install_runtime_tools.sh) usa fontes temporárias somente Ubuntu, assinadas pelo keyring Ubuntu, e preserva fontes globais e falhas estritas. Os três jobs com APT usam os mesmos perfis/pacotes fixados de antes. A simulação real do perfil completo de staging resolveu todos os pacotes sem instalá-los; o run hospedado citado acima confirmou o setup. Passaram **130 regressões Python** e novamente os **2 ktests gráficos**, sem mudar seus gates. Evidência em `artifacts/framebuffer-contract-1900/`, `artifacts/framebuffer-hardware-1900/` e `artifacts/cycle-1900/`. Não houve nova qualificação GPUI, DRM ou hardware físico.

## Instalador: nomes de partições NVMe, eMMC e NBD

O [instalador da ISO](../asterinas/distro/aster_nixos_installer/templates/aster-nixos-install) só inseria `p` para discos loop. Para `/dev/nvme0n1` ele construía **`/dev/nvme0n11` e `/dev/nvme0n12`**, em vez de `/dev/nvme0n1p1` e `/dev/nvme0n1p2`. O mesmo erro afetava nomes diretos eMMC e NBD. A função pura `set_partition_devices` agora seleciona esses nomes, preservando as regras existentes de SCSI/SATA (`sda1`), virtio (`vda1`), Xen (`xvda1`) e loop (`loop0p1`). Não altera particionamento, formatação, mounts ou aceitação da instalação.

A prova antes/depois executou **somente a regra shell real**, extraída do template antigo e do corrigido: NVMe/eMMC/NBD produziram o caminho incorreto antes e correto depois; loop/virtio mantiveram o resultado. Nenhum dispositivo foi aberto ou formatado. As regressões executam a função atual isoladamente e verificam sua chamada antes de consultar as partições. Isso **não demonstra detecção, instalação nem boot em controladora NVMe/eMMC/NBD**; aliases `/dev/disk/by-*`, multipath e entrada apontando para uma partição não foram qualificados.

Na [ISO de `34dd887`, run 39701305949](https://github.com/cybermanju/cybermanju-shell/actions/runs/39701305949), o serial chegou a `Installation finished. No error reported.` e `installation finished!`, mas o harness expirou esperando o marcador final do wrapper. O último desses eventos ocorreu cerca de seis segundos antes do timeout. Entre ele e o marcador final, o fonte executa exclusão de gerações antigas e GC do Nix store do alvo; o log não identifica qual etapa terminou ou ficou pendente. Foram acrescentadas mensagens **antes** dessas duas tarefas para tornar o próximo diagnóstico discriminante; elas não contam como sucesso. O deadline, o marcador final, a verificação EFI GRUB e o boot instalado não foram relaxados.

Logs e comparação shell estão em `artifacts/cycle-2330/iso-diagnosis/` e `artifacts/cycle-2330/partition-names-before-after.json`. A falha observada usa virtio e **não é explicada pela correção NVMe**. O boot do Asterinas instalado foi pulado; mensagens do instalador Linux não o comprovam. O [CI da base `23acc50`](https://github.com/cybermanju/cybermanju-shell/actions/runs/39709612337) concluiu os sete jobs de regressão, incluindo os FileOps de fbdev; sua ISO seguia em execução na consulta deste ciclo. Isso não aprova automaticamente esta nova alteração do instalador.

Validação local desta alteração: **180 regressões Python**, incluindo execução isolada dos oito exemplos de nomes e ordem estática dos diagnósticos; `bash -n` do template e diff sem erros. Também passaram novamente **três ktests gráficos em VMs separadas** (contrato de boot, framebuffer/console e FileOps de fbdev), em x86-64/q35/TCG com QEMU 8.2.2 e Rust `nightly-2026-07-21`. São regressões do kernel inalterado, **não testes do instalador em NVMe**. Seriais, JSON e proveniência em `artifacts/fbdev-2330/`, `artifacts/framebuffer-contract-2330/`, `artifacts/framebuffer-hardware-2330/`; resumo host em `artifacts/cycle-2330/python-tests.log`. Nenhuma nova versão de Linux, módulo `.ko` ou arquitetura foi qualificada.

## Pipeline da ISO: bloqueio identificado

A [execução de ISO em `4133000`](https://github.com/cybermanju/cybermanju-shell/actions/runs/39656869402) falhou **antes do build**, no timeout de dois minutos para remover SDKs efêmeros e medir capacidade. O log chegou a mostrar cerca de 102 GiB disponíveis; não foi evidência de falta de disco nem falha de boot do kernel.

O passo passa a ter dez minutos. Para preservar a invariável já testada de que a soma dos limites individuais cabe no job, o limite global passa a 358 minutos: soma de 356, ambos abaixo do cap hospedado de 360. Reserva de 1 GiB e gates de instalação/boot permanecem. Testes estáticos validam esses limites. A [execução posterior em `fb5b9d0`](https://github.com/cybermanju/cybermanju-shell/actions/runs/39658861099) superou preparação, build, verificação da ISO e live USB, mas depois falhou por timeout de 3.600 segundos esperando sucesso da instalação em TCG. O console terminava em cópia de paths Nix, sem mensagem de conclusão; o boot instalado não foi testado. Diagnósticos em `artifacts/iso-diagnosis-1730/`. O run seguinte de `540a0b8` também terminou por timeout de 3.600 segundos no marcador de instalação concluída, ainda copiando paths Nix; seus logs estão em `artifacts/iso-diagnosis-1900/`. A ISO de `9b31616` iniciou depois. Nenhum gate foi relaxado. Isso comprova apenas o desbloqueio inicial, não aceite completo da ISO.

## O que ainda falta

O consumidor ainda recusa endereço físico não alinhado a página. Os formatos de cor são inferidos por bpp; máscaras e ordem de canais não são preservadas. Linux boot, Multiboot v1, 8/16/24 bpp e GPU física não receberam nova qualificação em runtime.

No caminho gráfico faltam formato completo de cor, mapeamento com offset de página, apresentação DRM/input e integração GPUI↔kernel de ponta a ponta. No loader há agora [reserva virtual real testada](linux-module-compatibility.md), agora ligada ao replanejamento e à preparação de dados de uma fixture; já existe readback por mapping temporário somente em ktest de uma CPU; faltam mapping persistente de produção/SMP, proteção considerando o alias linear e chamada controlada de init. Readback de bytes não comprova cores, sincronização de apresentação ou FPS.
