# Gráficos e hardware: o que já foi provado

**O console agora dimensiona seu buffer e seu scroll pelo span lógico da superfície, corrigindo dois panics reproduzidos em QEMU.** O framebuffer mantém as escritas limitadas ao span e a prova de readback. A medição real continua em **1280 × 800, 32 bpp, pitch de 5.120 bytes e superfície de 4.096.000 bytes**. Isso não demonstra GPUI nativo, aceleração gráfica ou imagem apresentada em monitor físico.

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
| Adaptadores de produção | `cargo check --locked -p ostd -p linux-module-staging -p aster-framebuffer --target x86_64-unknown-none` aprovado | Type-check, não boot do kernel completo |
| Regressões históricas da etapa de limites | 109 testes Python, 115 testes ELF e 15 testes host de staging aprovados | Não executam init/exit de `.ko` |
| CI anterior `4133000` | [Sete jobs de regressão aprovados](https://github.com/cybermanju/cybermanju-shell/actions/runs/39656869405), incluindo framebuffer e staging real | Evidência daquela revisão, não aprovação automática desta mudança |

As VMs são imagens mínimas OSTD, **não a ISO instaladora nem a instalação UEFI completa**. Nenhum módulo, versão Linux ou arquitetura adicional foi qualificado nesta mudança; a [matriz de módulos](linux-module-compatibility.md) mantém as fixtures e suas restrições.

Reprodução a partir da raiz:

```sh
bash tools/ci/test_framebuffer_runtime.sh artifacts/framebuffer-contract --kind contract
bash tools/ci/test_framebuffer_runtime.sh artifacts/framebuffer-hardware --kind hardware
```

O [runner](../tools/ci/test_framebuffer_runtime.sh) recusa configurações preexistentes, invalida sucesso antigo, limita a VM a 600 segundos e reconstrói só o bundle descartável. Os diretórios guardam serial, JSON verificado, GRUB extraído da imagem, manifests/lockfile, versões e hashes. O [verificador](../tools/ci/verify_framebuffer_runtime.py) agora exige também o marcador único dos seis limites; serial antigo sem essa prova não aprova a nova versão. Panics contraditórios também são recusados.

## Infraestrutura gráfica: correção confirmada no CI

O [CI de `ec4b7f2`, run 39681738837](https://github.com/cybermanju/cybermanju-shell/actions/runs/39681738837), aprovou seis dos sete jobs, mas o gráfico parou ao atualizar APT: `packages.microsoft.com` retornou HTTP 403. As etapas de contrato e MMIO foram puladas; isso não é uma regressão gráfica demonstrada. O log bruto está em `artifacts/cycle-1900/ci-failed.log`. Não se deve aprovar o gate ignorando o erro ou dispensando assinaturas.

O [CI posterior de `9142a04`, run 39684369567](https://github.com/cybermanju/cybermanju-shell/actions/runs/39684369567), **passou nos sete jobs**, inclusive as duas VMs gráficas. Isso confirma a correção de setup naquela revisão, não aprova automaticamente o novo código de console.

O novo [helper de instalação](../tools/ci/install_runtime_tools.sh) usa fontes temporárias somente Ubuntu, assinadas pelo keyring Ubuntu, e preserva fontes globais e falhas estritas. Os três jobs com APT usam os mesmos perfis/pacotes fixados de antes. A simulação real do perfil completo de staging resolveu todos os pacotes sem instalá-los; o run hospedado citado acima confirmou o setup. Passaram **130 regressões Python** e novamente os **2 ktests gráficos**, sem mudar seus gates. Evidência em `artifacts/framebuffer-contract-1900/`, `artifacts/framebuffer-hardware-1900/` e `artifacts/cycle-1900/`. Não houve nova qualificação GPUI, DRM ou hardware físico.

## Pipeline da ISO: bloqueio identificado

A [execução de ISO em `4133000`](https://github.com/cybermanju/cybermanju-shell/actions/runs/39656869402) falhou **antes do build**, no timeout de dois minutos para remover SDKs efêmeros e medir capacidade. O log chegou a mostrar cerca de 102 GiB disponíveis; não foi evidência de falta de disco nem falha de boot do kernel.

O passo passa a ter dez minutos. Para preservar a invariável já testada de que a soma dos limites individuais cabe no job, o limite global passa a 358 minutos: soma de 356, ambos abaixo do cap hospedado de 360. Reserva de 1 GiB e gates de instalação/boot permanecem. Testes estáticos validam esses limites. A [execução posterior em `fb5b9d0`](https://github.com/cybermanju/cybermanju-shell/actions/runs/39658861099) superou preparação, build, verificação da ISO e live USB, mas depois falhou por timeout de 3.600 segundos esperando sucesso da instalação em TCG. O console terminava em cópia de paths Nix, sem mensagem de conclusão; o boot instalado não foi testado. Diagnósticos em `artifacts/iso-diagnosis-1730/`. O run seguinte de `540a0b8` também terminou por timeout de 3.600 segundos no marcador de instalação concluída, ainda copiando paths Nix; seus logs estão em `artifacts/iso-diagnosis-1900/`. A ISO de `9b31616` iniciou depois. Nenhum gate foi relaxado. Isso comprova apenas o desbloqueio inicial, não aceite completo da ISO.

## O que ainda falta

O consumidor ainda recusa endereço físico não alinhado a página. Os formatos de cor são inferidos por bpp; máscaras e ordem de canais não são preservadas. Linux boot, Multiboot v1, 8/16/24 bpp e GPU física não receberam nova qualificação em runtime.

No caminho gráfico faltam formato completo de cor, mapeamento com offset de página, apresentação DRM/input e integração GPUI↔kernel de ponta a ponta. No loader há agora [reserva virtual real testada](linux-module-compatibility.md), agora ligada ao replanejamento e à preparação de dados de uma fixture; já existe readback por mapping temporário somente em ktest de uma CPU; faltam mapping persistente de produção/SMP, proteção considerando o alias linear e chamada controlada de init. Readback de bytes não comprova cores, sincronização de apresentação ou FPS.
