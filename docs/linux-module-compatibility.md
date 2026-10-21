# Módulos Linux: o que funciona e o que falta provar

**Novo avanço: staging executado em OSTD/QEMU com frames reais. Um teste kernel passou pelos cinco cenários de cópia/leitura, zeros, limites e descarte em falhas injetadas. O runner confirmou 1 passou, 0 falhou e 256 filtrados. Nenhum `.ko` foi fornecido a essa VM ou executado; a ligação do ELF ao staging continua pendente.** A preparação anterior das duas fixtures `.ko` e do objeto C com páginas numéricas de 4 KiB foi preservada. A original mantém exports sintéticos; a auxiliar usa zero exports, mas flags não padrão e bypass de objtool exclusivo desse objeto. A meta permanece uma fixture real em QEMU/Asterinas, não compatibilidade universal.

Separadamente, a revisão anterior `47ee9b5` aprovou instalação e boot instalado com `hello` em QEMU x86-64/UEFI/TCG. O novo teste de staging usa uma imagem mínima OSTD distinta da ISO instaladora, BIOS/GRUB multiboot2 e TCG; não qualifica instalação, `.ko` ou hardware físico.

## Matriz atual

| Componente / alvo | Evidência | Limite |
|---|---|---|
| Asterinas do fork | `0.19.0`; base desta implementação `533e584` | Runtime mínimo de staging, não boot instalado desta revisão |
| Toolchain do kernel | Pin `nightly-2026-07-21`; Rust `1.99.0-nightly (87e5904f5 2026-07-20)`; Cargo `1.99.0-nightly (3efb1f477 2026-07-17)` | Type-check normal e link/boot do harness ktest x86-64; host usa `1.97.1` |
| `linux-module-elf` | **111 testes Rust**, `no_std`, sem dependências externas, `forbid(unsafe_code)` | Biblioteca de preparação offline; não integrada a syscalls ou execução |
| **OSTD `ModuleStaging`** | Backend real `Frame<()>`/`VmIo` exercitado em QEMU; **1 ktest / 5 cenários**, além dos **15 testes host** | Sem ELF no runtime, novo mapping, execução de módulo ou proteção W^X |
| VM do runtime | QEMU `8.2.2`, q35, TCG, `-cpu max`, 1 CPU e 1 GiB, BIOS/GRUB multiboot2 | Não usa KVM, rootfs externo, disco do usuário ou a ISO instaladora |
| Layout compacto anterior | `.ko` original: 16 seções / span 1.856 bytes; C: 6 / 320 bytes | Política antiga preservada; não garante isolamento de páginas |
| Layout de páginas | Base alinhada, 4 KiB fixos, páginas ocupadas disjuntas; recusa WRITE+EXEC inclusive vazio | Apenas números e flags; não reserva frames nem altera tabelas de páginas |
| Resolução + RELA | Planos compacto e paginado aceitos pelo resolvedor e pela transação completa | Exports reais, CRC/modversions e ABI não implementados |
| **`cybermanju_fixture.ko` original** | kbuild/MODPOST/link Linux `6.8.0-147-generic`, x86-64; 44 seções ELF; preparação abaixo | Imports `__fentry__`/`__x86_return_thunk` ainda recebem apenas valores sintéticos nos testes |
| **`cybermanju_noimports_fixture.ko` auxiliar** | Mesmo Linux/arquitetura, `nm -u` vazio; preparação com zero exports; init FUNC 4 bytes e exit FUNC 2 bytes identificados na inspeção compacta | Flags não padrão e objtool desviado nesse objeto; init/exit nunca chamados; segurança e ABI não demonstradas |
| Outros módulos/versões/arquiteturas | Nenhum módulo qualificado | Sem promessa para GPU, NVIDIA, ARM64, RISC-V ou LoongArch |
| NVIDIA NVport | Fonte `615.78.08`, pin `1a82fbfda8f321b056eb3568850f9316c5e465c5` previamente documentado | Ponte C/Rust por fontes, não `nvidia.ko`; sem nova validação de RM/GSP/GPU |

### Preparação real com a nova política

| Fixture | Seções ALLOC | Intervalos ocupados não vazios | Span numérico com padding | Payload de dados | Entradas RELA ALLOC |
|---|---:|---:|---:|---:|---:|
| Objeto C, **não módulo Linux** | 6 | 5 | 20.480 bytes | 310 bytes | 4 |
| `.ko` original | 16 | 13 | 53.248 bytes | 1.794 bytes | 13 |
| `.ko` auxiliar sem imports | 13 | 10 | 40.960 bytes | 1.721 bytes | 6 |

O span não representa memória mapeada ou consumida pelo heap. O payload não inclui padding de endereços. Cada fixture foi preparada nas duas políticas; o [oracle compartilhado](../asterinas/kernel/libs/linux-module-elf/tests/page_probe.rs) recalcula alinhamento por divisão em `u128`, confere todas as seções ALLOC, todas as entradas RELA aplicadas e todos os bytes finais, incluindo partes não alteradas e NOTE opaco. Confere também orçamento de span insuficiente e rejeição de flags WRITE+EXEC. Não executa bytes do módulo.

## Staging em OSTD: ownership sem execução

[`ModuleStaging`](../asterinas/ostd/src/mm/module_staging.rs) recebe um span e chunks com offsets relativos, não endereços de kernel. Valida tamanho/alinhamento, limites de páginas/chunks, intervalos e sobreposição antes de alocar qualquer frame. Adquire **todas** as páginas antes da primeira cópia; falha de alocação/escrita retorna erro e descarta todos os handles adquiridos, sem devolver imagem parcial. O owner não implementa Clone nem expõe handles, ponteiros ou escrita após a construção. Leitura confere overflow/limites; erro do backend pode deixar prefixo no buffer de saída.

O adaptador usa `FrameAllocOptions::new().zeroed(true).alloc_frame()` e `VmIo` reais. Aloca o **span inteiro, inclusive gaps**, não só o payload ou intervalos ocupados; `max_pages` limita essa quantidade. Gaps e caudas começam zerados pelo contrato do alocador. A liberação descarta referências; não há scrubbing explícito. Exige heap e alocador físico já inicializados. Este módulo proíbe `unsafe` e não depende da biblioteca ELF em `kernel/`.

Os [15 testes de ownership](../asterinas/ostd/src/mm/module_staging/storage_tests.rs) usam páginas falsas instrumentadas: verificam cópia entre páginas, gaps/caudas, entradas vazias, validação tardia antes de qualquer alocação, todos os pontos de falha de alocação e cópia, descarte no sucesso, inputs preservados, overflow e erros de leitura. Eles compilam o núcleo de armazenamento e o enum de erros reais de OSTD; o `PageTableError` do harness é só um tipo placeholder, não uma tabela de páginas. **Não testam alocação física, MMU, NX ou execução.**

### O que o teste kernel agora demonstra

O [teste nativo](../asterinas/ostd/src/mm/module_staging/runtime_tests.rs) usa o mesmo core e alocador real após inicialização de OSTD. Exercita o wrapper público `Frame<()>`: copia um payload de uma página mais 13 bytes, outro em offset de três páginas e lê o span de cinco páginas; confere zeros em gaps/caudas. Os demais cenários verificam vazio/limites, erro de alocação injetado após dois frames reais, erro de escrita injetado depois de adquirir os quatro frames e manutenção do ownership até Drop. Os erros são injetados, **não** exaustão real da RAM nem falha de MMU.

Nos cenários instrumentados, frames reais carregam [`TrackedMetadata`](../asterinas/ostd/src/mm/module_staging_test_meta.rs), com contador por teste acionado quando sua metadata é descartada. Os contadores confirmam 2 ou 4 últimos handles descartados nos pontos esperados; não inspecionam a free-list nem provam reutilização física, scrubbing ou recuperação sob pressão. Esse helper só existe sob `cfg(ktest)`; a macro de metadata fica fora da fronteira `forbid(unsafe_code)` do staging. Não houve mudança da API ou lógica de produção.

Há **um** `#[ktest]` agregador e **cinco cenários**, não cinco testes independentes. A ajuda de OSDK sugere substring, mas o runner implementa sufixo exato de caminho: selecionar apenas `module_staging` executou zero testes e não foi aceito como evidência. O executor agora seleciona `cybermanju_staging_runtime`, exige os cinco marcadores e `1 passed; 0 failed`, rejeitando resultados vazios, duplicados ou falhos. Entrada stdin fechada evita SIGTTIN/job-control observado na primeira tentativa local; essa tentativa interrompida também não conta como aprovação.

[`test_module_staging_runtime.sh`](../tools/ci/test_module_staging_runtime.sh) compila OSDK com **`OSDK_LOCAL_DEV=1`**, essencial para usar o fork e helpers locais, não pacotes publicados. Recusa substituir uma configuração OSTD existente, remove a temporária ao sair e invalida sucesso antigo antes de tentar novamente. Guarda serial, versões, hashes e Cargo.toml/lockfile efetivamente resolvidos do harness. O lockfile desse harness é gerado pelo OSDK, não uma reprodução bit a bit do lockfile do workspace; o arquivo capturado identifica a resolução testada. O novo job `module-staging-runtime` repete a prova em TCG, separado dos testes host e do type-check.

**Bloqueio de proteção identificado no código existente:** o [mapeamento linear de OSTD](../asterinas/ostd/src/mm/kspace/mod.rs) é RW e continua presente. Criar outro alias RX não bastará para W^X físico. O mapeamento do próprio código do kernel também ainda usa RWX; isso é estado preexistente, não foi alterado. Ownership de handles privados não comprova exclusividade física nem remove aliases. O staging só usa o mapping linear existente; não certifica NX e sua API de leitura não é proteção read-only de hardware.

## O contrato das páginas

[`plan_page_layout`](../asterinas/kernel/libs/linux-module-elf/src/layout.rs) exige base alinhada a 4.096 e mantém a ordem dos índices de seção. Cada seção começa alinhada a `max(4096, sh_addralign)`. Uma seção não vazia ocupa numericamente até seu fim arredondado à próxima página; a seguinte não compartilha essas páginas, mesmo se tiver flags iguais. É uma política simples de isolamento, não um compactador otimizado.

Seções vazias não ocupam páginas e podem compartilhar endereço com outras, mas eventual gap de alinhamento ainda debita o orçamento. Padding inicial/intermediário e cauda arredondada entram em `max_span`; quantidade de seções e overflow têm verificações independentes. WRITE+EXEC é recusado mesmo em ALLOC vazio; seções não ALLOC não são planejadas. NOTE/NOBITS e os tipos admitidos anteriores permanecem; TLS ou tipos ALLOC não suportados são recusados.

`Layout::page_size()` informa `Some(4096)` ou `None` para o plano compacto. A política é privada, faz parte da igualdade estrutural e é recalculada pelo `SymbolResolver::new`; um plano paginado não é validado silenciosamente como compacto. Limites excedentes do chamador não alteram a igualdade de planos equivalentes. A validação continua estrutural, **não uma identidade criptográfica dos bytes**.

Flags do planejador indicam intenção R/RW/RX, mas nenhuma permissão é aplicada. Os endereços numéricos **não são ponteiros dos Vecs**; o planejador não valida endereço canônico/kernel, frames físicos, ownership, NX, TLB ou MMU. A política isoladamente não torna execução segura nem está ligada ao novo staging. O planejador reutiliza uma reserva falível de metadados; seus orçamentos não limitam todo overhead do alocador ou tempo de CPU.

## Preparação, símbolos e relocations

[`SymbolResolver::prepare_module`](../asterinas/kernel/libs/linux-module-elf/src/resolve.rs) prepara todas as seções ou retorna erro, sem resultado parcial. Faz preflight de payload, quantidade de seções e entradas ALLOC antes de reservar payload/patches. O teto agregado de entradas é 65.536, incluindo NONE; bytes de payload incluem NOTE/NOBITS, mas não padding de endereço, capacidade do alocador ou memória temporária. O span é limitado separadamente pelo planejador escolhido.

PROGBITS é copiado, NOBITS começa zerado, NOTE é copiado opaco e qualquer RELA direcionada a NOTE é recusada. Tabelas debug/não ALLOC não são aplicadas, embora `inspect` valide sua estrutura. Falha tardia descarta cópias privadas; entrada e resultados anteriores não mudam. [`PreparedModule`](../asterinas/kernel/libs/linux-module-elf/src/image.rs) só oferece getters imutáveis de dados e placements, não mappings ou uma autorização para executar.

Símbolos definidos usam endereço planejado + `st_value` verificado; ABS usa seu valor. COMMON e definições em seções não planejadas são recusados. Undefined global/weak exige export exato; weak ausente não vira zero. Zero só é aceito se fornecido explicitamente. Undefined local, símbolo nulo e tipos/bits/visibilidades não suportados são erros. Há teto de 4.096 exports, nomes não vazios/sem NUL/sem duplicação e busca ordenada. Não há descoberta de exports reais, política GPL, CRC/modversions ou ABI.

RELA usa `S + A` ou `S + A - P` em `i128`, exigindo representação exata no destino. Tipos aceitos: NONE, 64, PC64, PC32, PLT32, 32 e 32S; REL/RELR e demais tipos são recusados. PLT32 usa destino direto, sem criar PLT. Overlap entre tabelas e erros tardios não devolvem resultado parcial. As APIs anteriores continuam disponíveis; o total anterior de 2.909 escritas em 14 cópias incluindo debug **não** é o total ALLOC nem evidência de execução.

## Fixture auxiliar: desvio explícito, não compatibilidade padrão

A [fixture auxiliar](../asterinas/kernel/libs/linux-module-elf/tests/kbuild-noimports/cybermanju_noimports_fixture.c) tem init que retorna zero, exit vazio e `MODULE_INFO(cybermanju_test_only, ...)`. É um `.ko` produzido pelo kbuild, não um objeto C renomeado. A original foi preservada; nenhum flag global de kernel/driver foi alterado.

Seu [Makefile](../asterinas/kernel/libs/linux-module-elf/tests/kbuild-noimports/Makefile) remove `-pg -mfentry -mfunction-return=thunk-extern` e adiciona `-fno-instrument-functions -mfunction-return=keep` somente nesse objeto. `OBJECT_FILES_NON_STANDARD_... := y` desvia o objtool nele: sem o desvio, o objtool recusou esse perfil; sem remover o thunk, havia import de `__x86_return_thunk`. **Não é configuração padrão, prova de mitigação ou autorização para carregar/executar.** Vermagic identifica headers de origem, não certifica o perfil alterado.

O [executor auxiliar](../tools/ci/test_linux_module_noimports.sh) exige headers/release/arquitetura corretos, zero undefined e preparação com exports vazios. Guarda comando real do compilador, desassemblagem, `readelf`, `modinfo`, hashes e status. Seu probe confere entradas globais FUNC em seções marcadas executáveis/não graváveis e limites/endereço numérico, mas não as chama. Tornar init um import estruturalmente válido deve causar `MissingSymbol`, sem inventar destino. A ausência de imports também não prova que um código arbitrário seja seguro.

## Reprodução e identificação dos builds

Os builds locais preservados da etapa anterior usam headers Ubuntu **`6.8.0-147.147`**, **GCC `13.3.0-6ubuntu2~24.04.1`** (`x86_64-linux-gnu-gcc-13`) e Rust **`1.97.1`**. Não foram reconstruídos localmente nesta rodada de staging; os jobs kbuild existentes continuam a reconstruí-los por commit. Vermagic: `6.8.0-147-generic SMP preempt mod_unload modversions`. Na original foram observados R_X86_64_32, R_X86_64_64, R_X86_64_PC32 e R_X86_64_PLT32. BTF não foi gerado por falta do `vmlinux` correspondente.

| Artefato local preservado | SHA-256 |
|---|---|
| `.ko` original | `b7991d69e0314df9477ee248b2d6a6fff90c9b6c9acfc500f6d9ce1c7cf2138b` |
| Fonte C original | `ae0db544d38a724cdac33d74b0d54716628dd23d87bc613dae2f072dd72e3da7` |
| `.ko` auxiliar | `6c1909521a3b0c98932ec96af105be1c95247cc24f3a0f60606ace473a805ec9` |
| Fonte C auxiliar | `abb935fe59de4bd0ac561f4ef5c8ec41ffedc073542a994012cee876cfc605ab` |
| Makefile auxiliar | `b11eaaff663015157e74ff54ed94937208f476475d1b1d87e94cee4b69a0963d` |
| `.config` dos headers | `cf8454bc2e67effab87a5bff3c39042f2d8d106b6a777c4302e52f50d23b7400` |
| `Module.symvers` | `8e7eb5798789b9610c324b3d9b13373d88c30ae17bb9e41e622efd05e0277a87` |

O hash do `.ko` varia entre diretórios de build; cada amostra tem sua proveniência. Artefatos ignorados pelo Git em `artifacts/linux-module-kbuild/` e `artifacts/linux-module-noimports/`, ambos com `exit_code=0`, `loaded=false`. Logs anteriores em `artifacts/linux-module-pages/` e `artifacts/linux-module-ownership/`; runtime atual, resolução do harness e regressões em `artifacts/linux-module-runtime/`.

No último runtime local aprovado, SHA-256 do kernel ELF: `832ba48de8f249a4da3f4056618aa5766da0c1f73e781b4ceff80a839b5fa39b`; da imagem de teste: `65d1f4b8a604542266c7a6e65d51f546ead3ba468bcf731ee3c812037f43208d`. Não são hashes de `.ko` nem da ISO instaladora; timestamps da ISO, IDs de teste gerados por macro e caminhos podem variar por build. Os cinco marcadores e o resumo constam de `runtime.log`; `result.json` identifica escopo `ostd_frame_staging_only` e `linux_modules_loaded=false`. Logs e hashes são evidência operacional de testes, não atestação contra adulteração.

```bash
bash tools/ci/test_module_staging.sh
(cd asterinas/osdk && OSDK_LOCAL_DEV=1 cargo build --locked -p cargo-osdk)
bash tools/ci/test_module_staging_runtime.sh artifacts/linux-module-runtime
(cd asterinas && cargo check --locked -p ostd --target x86_64-unknown-none)
bash tools/ci/test_linux_module_elf.sh
bash tools/ci/test_linux_module_kbuild.sh artifacts/linux-module-kbuild
bash tools/ci/test_linux_module_noimports.sh artifacts/linux-module-noimports
python3 -m unittest discover -s tools/ci -p 'test_*.py' -v
python3 -m unittest discover -s gpui-shell/scripts -p 'test_frame_telemetry.py' -v
```

Nesta rodada passaram **1 teste kernel / 5 cenários, 15 testes host de staging, 111 testes ELF, 84 regressões CI/ISO e 19 de telemetria**, o probe C, sintaxe dos scripts e `git diff --check`. As 7 regressões novas cobrem evidência completa, ANSI/controles, cada marcador ausente, duplicação, compilação/zero testes em vez de runtime, resumo falho e preservação de configuração com invalidação de sucesso obsoleto. Type-check normal OSTD com features padrão passou novamente; o aviso preexistente de `iter_advance_by` permanece. A configuração sem default features não foi qualificada: na etapa anterior falhou em caminhos CVM preexistentes. O runtime focado não qualifica os outros **256 testes filtrados**. Testes finitos não são fuzzing exaustivo ou auditoria formal; conferir o CI remoto pelo SHA publicado.

## O que falta para chamar init

Ainda não há despacho de `init_module` (175), `delete_module` (176) ou `finit_module` (313) no caminho x86 previamente inspecionado. Acrescentar uma syscall e chamar bytes preparados não seria suficiente nem seguro. O primeiro teste executável deve ser interno e controlado em QEMU, com confiança na fixture, ABI exata, placement real e marcador serial observável, sem entrada arbitrária por syscall ou destinos fictícios.

**Próxima etapa:** ligar de forma checada os offsets de `PreparedModule` ao staging e demonstrar a cópia de uma fixture `.ko` real em OSTD/QEMU, ainda sem executar init. O plano numérico não é um endereço de execução real. Faltam controle de aliases/frames, placement/mapping protegido, sincronização/TLB e contrato de falha de init/unload. A transição futura para RX e dados R/RW deve considerar o alias linear RW, não só flags do novo mapping. `KVirtArea::map_frames` é interno a OSTD e não é uma API pronta de loader com essas garantias; VM de processo não resolve memória do kernel.

Execução nativa arbitrária em ring 0 não pode virar API segura só porque o ELF passou. A fronteira de confiança e execução de baixo nível pertencem a **OSTD**; `kernel/` e esta biblioteca continuam sem `unsafe`. Permanecem pendentes ABI/modversions, assinatura/confiança, privilégios e ciclo de vida. A fixture auxiliar reduz uma dependência do primeiro protótipo, não implementa os exports da original nem certifica drivers Linux arbitrários.

## Evidência de CI e boot

A base `533e584` teve o [CI 39641696396](https://github.com/cybermanju/cybermanju-shell/actions/runs/39641696396) aprovado, incluindo o staging host e type-check real. O novo runtime local foi validado separadamente; aprovação remota dessa base não transfere aprovação ao novo job. A consulta confirmou main sincronizada, nenhuma PR aberta e branch histórica já incorporada; nenhum merge repetido foi necessário. O [CI ISO 39636467347](https://github.com/cybermanju/cybermanju-shell/actions/runs/39636467347), revisão `8643c76`, estava na instalação UEFI e foi preservado; não há conclusão nova de boot instalado nesta rodada.

**Segundo run completo aprovado:** o [CI ISO 39623025465](https://github.com/cybermanju/cybermanju-shell/actions/runs/39623025465), revisão `47ee9b5`, concluiu com sucesso live USB, instalação/verificação EFI GRUB e boot instalado. O `boot.log` baixado contém `Test case 'hello' passed` e `PASS: installed Asterinas booted and hello-asterinas passed (tcg)`. Logs em `artifacts/iso-39623025465/`; não foi preciso baixar a ISO inteira. É outra revisão anterior, QEMU x86-64/UEFI/TCG, não prova do staging ou de `.ko`.

O [run ISO 39608968083](https://github.com/cybermanju/cybermanju-shell/actions/runs/39608968083), base `3d6dc25`, comprovou live USB, instalação UEFI CD/EFI GRUB e boot instalado. `boot.log` registra `PASS: installed Asterinas booted and hello-asterinas passed (tcg)` e **1 passou / 0 falhou**; serial registra EFI/OSTD. Evidência local em `artifacts/iso-39608968083/`. Foi QEMU/TCG, não hardware físico nem teste de módulos. O run antigo `39605334894` falhou na prontidão serial antes desses gates, sem causa isolada.

A ISO mantém `cancel-in-progress: false`; nenhum agendamento foi criado ou alterado nesta rodada.
