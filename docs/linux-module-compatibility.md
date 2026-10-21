# Módulos Linux: o que funciona e o que falta provar

**Avanço mais recente: a recusa do protótipo em topologia de duas CPUs passou em um ktest real de QEMU.** Três leituras válidas foram recusadas sem alterar destinos, conteúdo dos frames ou PTEs observados, e a reserva voltou ao alocador. Isso prova o bloqueio, **não suporte de mapping SMP**.

**Etapa de mapping temporário:** a fixture auxiliar foi lida pela própria base virtual reservada em um mapping RW temporário real. Os **40.960 bytes / 10 páginas** coincidiram com o ELF preparado. O protótipo existe **somente em ktest x86-64 com uma CPU**: desmonta o alias e invalida o TLB local antes de devolver o controle. Um controle negativo sem INVLPG copiou bytes estrangeiros e falhou como esperado. **Não há mapping de produção/persistente, init/exit, execução de módulo, NX global ou W^X demonstrados.**

**Etapa anterior:** uma fixture `.ko` real foi preparada dentro de OSTD/QEMU e suas seções copiadas para frames reais. A ponte conferiu **13 seções, 40.960 bytes de span, 1.721 bytes de payload, 6 entradas RELA e 10 páginas**, com leitura de todo o span idêntica ao esperado. Um ktest passou, zero falhou; conferiu também budgets insuficientes, layout divergente e política compacta recusados. **Init/exit não foram chamados e os endereços das relocations continuam numéricos, não placement real para execução.** É uma fixture auxiliar Linux `6.8.0-147-generic` x86-64 sem imports, com flags não padrão e bypass de objtool exclusivo do objeto; não é um driver qualificado ou compatibilidade universal.

Separadamente, a revisão anterior `47ee9b5` aprovou instalação e boot instalado com `hello` em QEMU x86-64/UEFI/TCG. O novo teste de staging usa uma imagem mínima OSTD distinta da ISO instaladora, BIOS/GRUB multiboot2 e TCG; não qualifica instalação, execução de módulos ou hardware físico.

**Avanço gráfico posterior:** o [contrato de framebuffer e o readback MMIO em QEMU](hardware-graphics-compatibility.md) passaram em VMs mínimas; o staging bruto foi revalidado com o novo header Multiboot2. Essa prova não executa `.ko`, não resolve placement/W^X e não revalida o boot instalado desta revisão. As contagens e fixtures abaixo descrevem a rodada de integração ELF→frames, não uma nova qualificação universal.

**Integração gráfica mais recente:** o [console de framebuffer](hardware-graphics-compatibility.md) foi corrigido para não confundir span lógico com capacidade MMIO. Dois controles negativos reproduziram panics de ativação e scroll; os três caminhos públicos testados passaram após a correção, com bytes de guarda intactos. Passaram 2 ktests gráficos, 131 regressões e type-check. Isso não muda o loader, ABI ou a lista de módulos/versões/arquiteturas qualificadas.

## Matriz atual

| Componente / alvo | Evidência | Limite |
|---|---|---|
| Asterinas do fork | `0.19.0`; protótipo ktest desenvolvido a partir de `9b31616` | Runtimes mínimos, não boot instalado desta revisão |
| Toolchain do kernel | Pin `nightly-2026-07-21`; Rust `1.99.0-nightly (87e5904f5 2026-07-20)`; Cargo `1.99.0-nightly (3efb1f477 2026-07-17)` | Type-check normal e link/boot do harness ktest x86-64; host usa `1.97.1` |
| `linux-module-elf` | **120 testes Rust**, `no_std`, sem dependências externas, `forbid(unsafe_code)` | Preparação de dados no host e no ktest; sem syscalls/init |
| **`linux-module-staging`** | **1 ktest** com bytes preparados, owner conjunto, 3 casos de ciclo de vida e readback pela base real em 10 páginas temporárias | Apenas ktest de uma CPU; sem mapping persistente/produção, syscalls ou init/exit |
| **OSTD `ModuleStaging`** | Backend real `Frame<()>`/`VmIo`; **1 ktest / 7 cenários** bruto, incluindo owner, mapping temporário, colisão e tradução stale, mais o novo ktest com ELF e **15 testes host** | Mapping experimental só em ktest de uma CPU; sem execução, SMP ou W^X |
| **Recusa SMP do protótipo OSTD** | 1 ktest, topologia real reportada de 2 CPUs, 3 chamadas válidas recusadas, 3 páginas de VA sem PTEs e faixa reutilizada após Drop | Somente bloqueio; não testa acesso concorrente, shootdown remoto, W^X ou init |
| **OSTD `ModuleAddressReservation`** | API x86-64; reserva RAII compartilhada com `KVirtArea`; **1 ktest / 4 cenários**, 2 e 3 páginas reservadas mais 1 página de controle | Primitiva sem frames/mapping; usada pela ponte de layout, sem garantia geral de alcance de relocations |
| VM do runtime | QEMU `8.2.2`, q35, TCG, `-cpu max`, 1 GiB, BIOS/GRUB multiboot2; mapping positivo em 1 CPU e recusa em 2 CPUs | Não usa KVM, rootfs externo, disco do usuário ou a ISO instaladora |
| Framebuffer, avanço posterior | x86-64, VGA virtual 1280×800×32; readback e **6 verificações de limites** no mesmo ktest; controle negativo falhou antes da correção | Superfície curta sintética sobre MMIO real; não executa `.ko`, não prova GPUI/scanout; [detalhes e estado da ISO](hardware-graphics-compatibility.md) |
| Layout compacto anterior | `.ko` original: 16 seções / span 1.856 bytes; C: 6 / 320 bytes | Política antiga preservada; não garante isolamento de páginas |
| Layout de páginas | Base alinhada, 4 KiB fixos, páginas ocupadas disjuntas; recusa WRITE+EXEC inclusive vazio | Apenas números e flags; não reserva frames nem altera tabelas de páginas |
| Resolução + RELA | Planos compacto e paginado aceitos pelo resolvedor e pela transação completa | Exports reais, CRC/modversions e ABI não implementados |
| **`cybermanju_fixture.ko` original** | kbuild/MODPOST/link Linux `6.8.0-147-generic`, x86-64; 44 seções ELF; preparação abaixo | Imports `__fentry__`/`__x86_return_thunk` ainda recebem apenas valores sintéticos nos testes |
| **`cybermanju_noimports_fixture.ko` auxiliar** | Mesmo Linux/arquitetura, `nm -u` vazio; preparação com zero exports; copiada para 10 frames no ktest; init FUNC 4 bytes e exit FUNC 2 bytes identificados na inspeção compacta | Flags não padrão e objtool desviado nesse objeto; init/exit nunca chamados; segurança e ABI não demonstradas |
| Outros módulos/versões/arquiteturas | Nenhum módulo qualificado | Sem promessa para GPU, NVIDIA, ARM64, RISC-V ou LoongArch |
| NVIDIA NVport | Fonte `615.78.08`, pin `1a82fbfda8f321b056eb3568850f9316c5e465c5` previamente documentado | Ponte C/Rust por fontes, não `nvidia.ko`; sem nova validação de RM/GSP/GPU |

## Recusa explícita em topologia de duas CPUs

O [novo ktest](../asterinas/ostd/src/mm/module_staging/runtime_tests.rs) exige `num_cpus() == 2`, cria dois frames com payload conhecido e reserva três páginas virtuais. Tenta leitura inicial, leitura cruzando a fronteira de página e leitura vazia no fim do span: os três pedidos são válidos para o protótipo de uma CPU, mas devem retornar `AccessDenied` nesta topologia. Os dois buffers de destino permanecem intactos; o conteúdo integral dos frames permanece igual. Consultas às tabelas reais verificam a ausência de PTEs nas três páginas antes/depois das recusas. Após Drop, a mesma faixa volta a ser reservada.

A [configuração dedicada](../tools/ci/module-mapping-smp-rejection.OSDK.toml) usa q35/TCG e `-smp 2`. O teste passou com uma única conclusão `1 passed; 0 failed`. **Não há acesso de módulo em paralelo entre CPUs, mapping persistente, PTE executável, shootdown remoto ou chamada de init.** Nenhum mapping ou API de produção foi introduzido; o adaptador continua restrito a ktest x86-64 e uma CPU para leituras mapeadas positivas.

**Controle negativo sem instalar mappings:** um bypass temporário que aceitava a leitura vazia antes de checar a topologia fez o ktest falhar (`Ok(())` em vez de `Err(AccessDenied)`). O backend original foi restaurado e a VM positiva repetida. Serial/JSON/hashes/manifests em `artifacts/module-smp-rejection-runtime/`; negativo em `artifacts/module-smp-empty-bypass-negative/`; logs em `artifacts/cycle-1930/`.

O [gate](../tools/ci/verify_module_smp_rejection_runtime.py) exige três marcadores completos únicos e um resumo isolado de um teste aprovado. Recusa CPU divergente, marcadores ausentes/estranhos/ecoados, panic/FAIL e resumos contraditórios. O JSON registra `pages_mapped=false`, `init_called=false`, `smp_mapping_supported=false` e `production_mapping_available=false`. O preflight também preserva arquivos/symlinks próprios e invalida sucesso antigo. Stubs de preflight não são evidência de VM.

Passaram **3 ktests em VMs separadas**: recusa em duas CPUs, regressão bruta em uma CPU com sete cenários e reserva com quatro cenários. Passaram **140 testes Python**, rustfmt, sintaxe dos scripts e diff. O CI continua com sete jobs; a nova VM é um passo obrigatório no job de runtime de módulos, com logs retidos. Não houve nova fixture `.ko`, versão Linux ou arquitetura qualificada.

## Primeira leitura pela base real: protótipo de uma CPU

O [backend OSTD](../asterinas/ostd/src/mm/kspace/module_staging_map.rs) só é compilado em `ktest` x86-64; `read_mapped_for_test(&mut self, ...)` **não existe na API de produção**. Também recusa topologias com mais de uma CPU. A recusa foi executada no novo teste de duas CPUs descrito acima; não é prova de mapping ou coerência de TLB em SMP. Não se tentou contornar o problema de shootdown remoto mantendo IRQs desabilitadas.

A chamada verifica limites/overflow e um teto de **1.024 páginas**, tanto de frames quanto de envelope. Mantém IRQs locais desabilitadas; confere a faixa vazia sob lock das tabelas, instala frames rastreados WB/RW/supervisor **sem o flag X** e invalida cada VA localmente, inclusive entradas GLOBAL. Lê pelo VA real, não pelo alias linear; o guard RAII remove só seu prefixo instalado e invalida novamente antes de liberar o CPU. O owner original conserva os frames e a reserva. Nenhum ponteiro, callback ou capacidade de execução é retornado. Alocações internas de tabelas/heap ainda podem panicar: isso não oferece OOM recuperável geral.

Durante cada cópia, asserções do harness inspecionam PTEs reais: frame físico esperado, RW, ausência de X/USER e cache WB. Conferem também que o slack fica sem mapping e que toda a reserva volta a ficar sem PTE após a chamada. Isso **não é teste de fault de execução NX**, remoção do alias linear, scrubbing ou garantia SMP.

O ktest OSTD passou leitura integral e parcial atravessando três páginas, gaps/zeros, overflow, leitura além do span, leitura vazia no fim, envelope acima do teto recusado e reutilização da faixa. A prova adversarial cria um mapping estrangeiro: a tentativa de leitura é recusada sem alterar o buffer ou remover esse PTE. Depois semeia uma tradução GLOBAL, remove seu PTE deliberadamente sem INVLPG, retendo o frame estrangeiro vivo, e exige que a chamada leia os frames próprios.

**Controle negativo real:** omitindo apenas o INVLPG anterior à cópia, a VM falhou com `stale GLOBAL translation must not supply foreign bytes`; o retorno continha bytes do frame estrangeiro. O fonte foi restaurado explicitamente e ambos os runtimes positivos foram repetidos com sucesso. Isso demonstra a necessidade dessa invalidação no cenário local testado, não uma prova geral de coerência de TLB.

A fixture auxiliar sem imports manteve **13 seções, 1.721 bytes de payload, 6 relocations e 10 páginas**; todos os 40.960 bytes lidos pela base `0xffffdffffff70000` coincidiram com o preparado. Não houve nova fixture, versão Linux ou arquitetura qualificada. Os gates exigem **7 marcadores brutos e 6 preparados**, únicos e completos; páginas do mapping devem coincidir com o layout, sem aceitar panic, echo ou resumos contraditórios. O JSON diz `pages_mapped=true`, mas `mapping_persists_after_read=false`, `production_mapping_available=false`, `mapping_scope=ktest_single_cpu_only` e `init_called=false`.

Passaram **2 ktests em VMs separadas, 123 testes Python, 120 testes ELF e 15 testes host de staging**, mais type-check normal de OSTD/ponte/framebuffer. Nenhum novo teste gráfico/GPUI ou de hardware físico nesta rodada. Serial, hashes e JSON em `artifacts/module-scoped-mapping-runtime/` e `artifacts/prepared-scoped-mapping-runtime/`; controle negativo em `artifacts/module-scoped-mapping-negative/` e logs em `artifacts/cycle-1830/`. Não são arquivos da ISO instaladora.

## Reserva virtual real: primitiva OSTD

[`ModuleAddressReservation`](../asterinas/ostd/src/mm/module_address.rs) usa um owner interno de faixa no mesmo alocador VMALLOC de `KVirtArea`, com IRQ local desabilitada durante alocação/liberação. Não é Clone e não oferece mapping ou ponteiro executável. Drop só devolve a faixa: não percorre tabelas de páginas. Exige span não vazio, múltiplo de 4 KiB e orçamento explícito de páginas **virtuais**, não de RAM. Falta de espaço contíguo retorna `NoMemory`; o bookkeeping `BTreeMap` existente não oferece recuperação de OOM de heap.

O [teste nativo](../asterinas/ostd/src/mm/module_address/runtime_tests.rs) aprovou seis casos de validação, reservas disjuntas de duas e três páginas e uma área vazia de uma página pela API `KVirtArea` anterior. Uma solicitação válida do pool inteiro falha enquanto essas áreas vivem. Consultas às tabelas reais não encontraram mappings nas seis páginas; após Drop, duas páginas foram reservadas novamente no primeiro endereço. Isso não prova coerência de TLB, NX ou reutilização de frames físicos.

Pool observado: `0xffffc00000000000..0xffffe00000000000`; primeira reserva: `0xffffdffffff78000..0xffffdffffff7a000`. São medições desta VM, não endereços fixos da API. Serial, JSON, manifests e hashes estão em `artifacts/module-address-runtime/`. O gate exige quatro marcadores completos únicos e um único resumo `1 passed; 0 failed`; rejeita serial parcial, misturado, contraditório ou com panic.

Na etapa anterior da reserva passaram **3 ktests em VMs separadas** (reserva, regressão de staging bruto e framebuffer), **117 testes Python, 115 testes ELF e 15 testes host de staging**, além de type-check de OSTD/ponte/framebuffer. Não foi acrescentado nenhum módulo ou versão Linux ao conjunto testado.

## Reserva e frames com ciclo de vida conjunto (etapa anterior)

[`ReservedModuleStaging`](../asterinas/ostd/src/mm/module_staging/reserved.rs), em OSTD/x86-64, reúne o staging e a reserva em um tipo sem Clone ou extração de partes. `ModuleStaging::with_reservation` consome ambos, exige span não vazio que caiba na faixa e aceita envelope maior. No sucesso e no erro de validação, os handles de frames são descartados antes da reserva. Unir owners não adquire novas páginas; o bookkeeping já existente ainda pode falhar por OOM de heap fora de Result. Não é mapping, scrubbing ou proteção MMU.

Na ponte segura, `ReservedModuleLayout::stage` consome a reserva, valida/copia o preparado e retorna [`ReservedPreparedModule`](../asterinas/kernel/libs/linux-module-staging/src/reserved.rs), com metadata e esse owner OSTD. Retorna só leitura, span, contagem de frames e coordenadas reservadas. Não devolve owner parcial no erro. Layout clonado continua apenas metadata, não posse da faixa nem autenticação da ELF.

O ktest OSTD acrescentou reserva de duas páginas para um único frame, leitura/zeros, limite pelo span real mesmo dentro do envelope, reserva concorrente disjunta e reutilização da faixa após Drop. Staging vazio ou de dois frames com uma reserva de uma página foi recusado; a faixa foi retomada após cada erro. A ordem de destruição está no código; o teste **não mede reutilização física ou limpeza dos frames**.

O ktest da fixture passou pelo novo owner, preservou o readback dos 40.960 bytes e verificou três casos: sucesso/leitura/Drop/reutilização, orçamento de staging insuficiente e layout preparado em outra base. Nos dois erros, a reserva consumida voltou ao alocador. O gate exige cinco marcadores completos; o bruto exige seis e agora rejeita também marcadores misturados/contraditórios, panic e resumos extras ou embutidos. Um controle negativo falhou nos serials inválidos antes da correção do gate.

Validação desta etapa: **2 ktests, 122 testes Python, 120 testes ELF e 15 testes host de staging**, mais type-check de OSTD/ponte/framebuffer. Não houve novo teste gráfico nem novo módulo/versão/arquitetura. Provas atuais em `artifacts/module-owner-runtime/`, `artifacts/prepared-owner-runtime/` e `artifacts/cycle-1800/`; as evidências nas seções seguintes são históricas quando indicadas.

## Da reserva ao layout e aos bytes preparados (etapa anterior)

[`page_layout_span_bound`](../asterinas/kernel/libs/linux-module-elf/src/layout.rs) calcula um envelope conservador antes de conhecer a base. Para cada seção, cobra o pior gap entre endereços alinhados a página (`max(4096, sh_addralign) - 4096`) e o tamanho não vazio arredondado. Vazios também podem cobrar alinhamento. A soma é checked, respeita orçamento e pode recusar casos que caberiam em uma fase mais favorável. Cinco testes host novos cobrem fases de alinhamento, budgets, validação preservada e overflow; não são medições de hardware.

[`ReservedModuleLayout`](../asterinas/kernel/libs/linux-module-staging/src/reserved.rs) mantém juntos a reserva OSTD e o layout **recalculado** em sua base. Não desloca cegamente um plano baixo. Rejeita orçamento vazio/irrepresentável e span zero; falha de replanejamento devolve a reserva via Drop. Layout clonado é apenas metadata e não retém o owner. O envelope pode conter espaço não usado pelo layout, e o budget virtual não limita todo o heap.

O ktest preparado manteve a prova baixa e acrescentou preparação real com exports vazios, comparação dos bytes que mudaram pelas relocations e leitura de todo o staging contra o novo preparado, incluindo gaps e caudas. A reserva ficou viva durante a operação. A fixture tem seis relocations ALLOC `R_X86_64_64`; isso **não qualifica imports, 32/32S, trampolines ou ABI Linux**. Serial/hashes/fixture em `artifacts/prepared-module-reserved-runtime/`; regressões e relocations em `artifacts/cycle-1730/`. Um ktest passou, zero falhou; também passaram **120 testes ELF, 119 testes Python, 15 testes host de staging** e type-check de OSTD/ponte/framebuffer.

VMALLOC alto não cabe automaticamente em `R_X86_64_32`/`32S`; PC32/PLT32 podem falhar para exports distantes. A preparação continua fail-closed e não cria trampolines. A nova ponte não muda permissões, TLB ou o alias linear RW. Não há novo módulo, versão Linux ou arquitetura qualificada.

## Preparação real das fixtures na política paginada

| Fixture | Seções ALLOC | Intervalos ocupados não vazios | Span numérico com padding | Payload de dados | Entradas RELA ALLOC |
|---|---:|---:|---:|---:|---:|
| Objeto C, **não módulo Linux** | 6 | 5 | 20.480 bytes | 310 bytes | 4 |
| `.ko` original | 16 | 13 | 53.248 bytes | 1.794 bytes | 13 |
| `.ko` auxiliar sem imports | 13 | 10 | 40.960 bytes | 1.721 bytes | 6 |

Na etapa de planejamento, span é só um número, não memória reservada. No novo runtime auxiliar ele corresponde aos **10 frames de staging**, incluindo padding, mas não a um mapping para execução. O payload não inclui padding de endereços. A preparação host anterior nas duas políticas foi preservada; o [oracle compartilhado](../asterinas/kernel/libs/linux-module-elf/tests/page_probe.rs) recalcula alinhamento por divisão em `u128`, confere seções ALLOC, entradas RELA e bytes finais, inclusive NOTE opaco. Não executa bytes do módulo.

## Ponte ELF→frames: o teste inicial

[`PreparedModule::plan_staging`](../asterinas/kernel/libs/linux-module-elf/src/staging.rs) só aceita a política de páginas de 4 KiB e exige correspondência de todas as placements, ordem, tamanhos de buffers, offsets relativos e intervalos. Valida representabilidade/overflow, span e limites antes de reservar descritores. `max_pages` inclui gaps/caudas; `max_chunks` inclui vazios e tem teto adicional de **4.096**. Não é identidade criptográfica dos bytes de origem. Os quatro testes host novos incluem NOTE/NOBITS, imagem vazia, mismatch de base/alinhamento, budgets, chunk vazio de fato e 4.097 chunks recusados mesmo com orçamento maior.

[`stage_prepared`](../asterinas/kernel/libs/linux-module-staging/src/lib.rs) vive numa biblioteca kernel separada, `no_std` e `forbid(unsafe_code)`, dependente de ELF e OSTD; **OSTD não passa a depender de `kernel/`**. Depois de validar o plano, reserva descritores OSTD de forma falível e chama `ModuleStaging::new`. Não expõe owner parcial em erros. Dois arrays temporários de descritores, buffers já preparados e overhead dos alocadores não são contabilizados como páginas de staging.

O [ktest da ponte](../asterinas/kernel/libs/linux-module-staging/src/runtime_tests.rs) embute a fixture criada pelo kbuild, resolve com exports vazios e prepara RELA **dentro da VM**. Recalcula offsets independentemente do array da ponte, compara todos os 40.960 bytes (payload, zeros e gaps), confere entrada preparada imutável e leitura fora do span recusada. Depois verifica rejeição de budgets insuficientes, layout em outra base e layout compacto. Um teste passou, zero falhou, zero filtrado na crate da ponte; as outras crates do runner foram explicitamente puladas, não qualificadas.

Na prova baixa anterior, os endereços foram planejados na base numérica `0x10000000`. A prova adicional agora usa a base real reservada descrita acima: **nenhuma das duas é endereço dos frames ou autorização de execução**. Bytes relocados são apenas dados lidos de volta. Não houve criação de ponteiro de função, chamada a símbolo ELF, registro de módulo, syscall ou novo mapping. A fixture original com imports continua usando apenas exports sintéticos na preparação host, não foi transferida para o teste kernel desta rodada.

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

Flags do planejador indicam intenção R/RW/RX, mas nenhuma permissão é aplicada. Os endereços numéricos **não são ponteiros dos Vecs ou frames**; o planejador não valida endereço canônico/kernel, NX, TLB ou MMU. A nova ponte usa somente offsets relativos e ownership de staging, não transforma os números em placement real para execução. Orçamentos não limitam todo overhead do alocador ou tempo de CPU.

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

Os builds anteriores preservados e a reconstrução auxiliar desta rodada usam headers Ubuntu **`6.8.0-147.147`**, **GCC `13.3.0-6ubuntu2~24.04.1`** (`x86_64-linux-gnu-gcc-13`); a inspeção host local usou Rust **`1.97.1`**. O kernel de teste usa o nightly acima; o CI registra a versão efetiva de cada etapa. Vermagic: `6.8.0-147-generic SMP preempt mod_unload modversions`. Na original foram observados R_X86_64_32, R_X86_64_64, R_X86_64_PC32 e R_X86_64_PLT32. BTF não foi gerado por falta do `vmlinux` correspondente.

| Artefato local preservado | SHA-256 |
|---|---|
| `.ko` original | `b7991d69e0314df9477ee248b2d6a6fff90c9b6c9acfc500f6d9ce1c7cf2138b` |
| Fonte C original | `ae0db544d38a724cdac33d74b0d54716628dd23d87bc613dae2f072dd72e3da7` |
| `.ko` auxiliar | `6c1909521a3b0c98932ec96af105be1c95247cc24f3a0f60606ace473a805ec9` |
| Fonte C auxiliar | `abb935fe59de4bd0ac561f4ef5c8ec41ffedc073542a994012cee876cfc605ab` |
| Makefile auxiliar | `b11eaaff663015157e74ff54ed94937208f476475d1b1d87e94cee4b69a0963d` |
| `.config` dos headers | `cf8454bc2e67effab87a5bff3c39042f2d8d106b6a777c4302e52f50d23b7400` |
| `Module.symvers` | `8e7eb5798789b9610c324b3d9b13373d88c30ae17bb9e41e622efd05e0277a87` |

O hash do `.ko` varia entre diretórios de build; cada amostra tem sua proveniência. Os hashes da tabela identificam builds anteriores preservados, não a nova cópia runtime. Esses artefatos continuam em `artifacts/linux-module-kbuild/` e `artifacts/linux-module-noimports/`. Logs brutos em `artifacts/linux-module-runtime/`; nova fixture, serial e resolução do harness em `artifacts/prepared-module-runtime/`; regressões desta rodada em `artifacts/prepared-module-validation/`.

Na primeira rodada de runtime preparado local aprovada: SHA-256 da `.ko` embutida **`fe0bea181af95bca756e2e7423088aca381a6bdf868496a4e960a0d89bfe94d0`**; kernel ELF `b4b507b0d9fec184a3ebbcd7597aa3e3b5000c97892b6cb0594826d67ccf322d`; imagem de teste `3683169d106307f24325c422b6702a0d4a79c594517e1586e92c04e389a5749d`. Não são hashes da ISO instaladora. Timestamps, IDs de macro e caminhos variam por build. `result.json` declara `prepared_module_frame_staging_only`, `fixture_staged=true`, `linux_modules_executed=false` e `init_called=false`; logs/hashes não são atestação contra adulteração.

```bash
bash tools/ci/test_module_staging.sh
(cd asterinas/osdk && OSDK_LOCAL_DEV=1 cargo build --locked -p cargo-osdk)
bash tools/ci/test_module_staging_runtime.sh artifacts/linux-module-runtime
bash tools/ci/test_module_staging_runtime.sh artifacts/module-address-runtime --address-reservation
bash tools/ci/test_module_staging_runtime.sh artifacts/module-smp-rejection-runtime --smp-rejection
bash tools/ci/test_prepared_module_runtime.sh artifacts/prepared-module-runtime
(cd asterinas && cargo check --locked -p ostd -p linux-module-staging --target x86_64-unknown-none)
bash tools/ci/test_linux_module_elf.sh
bash tools/ci/test_linux_module_kbuild.sh artifacts/linux-module-kbuild
bash tools/ci/test_linux_module_noimports.sh artifacts/linux-module-noimports
python3 -m unittest discover -s tools/ci -p 'test_*.py' -v
python3 -m unittest discover -s gpui-shell/scripts -p 'test_frame_telemetry.py' -v
```

Na rodada anterior de integração ELF→frames passaram **2 ktests em VMs separadas** (o bruto anterior com cinco cenários e o novo com a fixture), **15 testes host de staging, 115 testes ELF, 97 regressões CI/ISO e 19 de telemetria**, o probe C, reconstrução/inspeção auxiliar, sintaxe dos scripts e `git diff --check`. As 13 regressões Python novas verificam o serial preparado, marcadores contraditórios/resumos múltiplos e preflight (modo inválido, fixture ausente, symlink quebrado, corrida de configuração com stubs). Stubs de preflight não são evidência de VM. O executor compartilhado agora recusa configuração criada durante o build por noclobber atômico e preserva mudanças de conteúdo na limpeza. Type-check normal de OSTD e da ponte passou, com o aviso preexistente de `iter_advance_by`. Testes focados não qualificam as crates puladas ou os 256 testes filtrados no kernel bruto. Não há fuzzing exaustivo ou auditoria formal; conferir o CI remoto pelo SHA publicado.

## O que falta para chamar init

Ainda não há despacho de `init_module` (175), `delete_module` (176) ou `finit_module` (313) no caminho x86 previamente inspecionado. Acrescentar uma syscall e chamar bytes preparados não seria suficiente nem seguro. O primeiro teste executável deve ser interno e controlado em QEMU, com confiança na fixture, ABI exata, placement real e marcador serial observável, sem entrada arbitrária por syscall ou destinos fictícios.

**Próxima etapa:** desenhar o contrato de mapping persistente e liberação diferida em SMP antes de expor uma API de produção. O protótipo acima é deliberadamente local e temporário; não pode simplesmente perder o guard de IRQ ou a recusa multi-CPU. Na API de produção, a reserva continua ligada ao layout e à preparação, mas não há frames persistentemente mapeados nela. A transição posterior para RX e dados R/RW precisa tratar o alias linear RW, não só flags do novo mapping, além de sincronização/TLB e falha de init/unload. `KVirtArea::map_frames` é interno a OSTD, não uma API pronta de loader com essas garantias; VM de processo não resolve memória do kernel. A fixture data-only atual não autoriza execução.

O [flusher atual](../asterinas/ostd/src/mm/tlb.rs) exige cuidado adicional antes de ser usado nesse caminho: `for_range` e a fila agregam 32 ou mais páginas em `for_all`, que explicitamente exclui entradas GLOBAL. Portanto, não é substituto direto do INVLPG por página usado no protótipo. Esperar shootdown remoto com IRQs desabilitadas também é vedado. São restrições de arquitetura ainda pendentes, não suporte SMP implementado.

Execução nativa arbitrária em ring 0 não pode virar API segura só porque o ELF passou. A fronteira de confiança e execução de baixo nível pertencem a **OSTD**; `kernel/` e esta biblioteca continuam sem `unsafe`. Permanecem pendentes ABI/modversions, assinatura/confiança, privilégios e ciclo de vida. A fixture auxiliar reduz uma dependência do primeiro protótipo, não implementa os exports da original nem certifica drivers Linux arbitrários.

## Evidência de CI e boot

O [CI de `4f8933b`, run 39686044422](https://github.com/cybermanju/cybermanju-shell/actions/runs/39686044422), **passou nos sete jobs**, incluindo os novos casos de console e os runtimes de módulos. A ISO dessa revisão estava pendente na consulta inicial. Essa aprovação é da base anterior; a nova recusa SMP tem evidência local e aguarda seu próprio CI após publicação.

O [CI de `9142a04`, run 39684369567](https://github.com/cybermanju/cybermanju-shell/actions/runs/39684369567), **passou nos sete jobs**, incluindo framebuffer e runtimes de módulos. Sua ISO estava pendente na consulta desta rodada. A correção de console posterior foi validada localmente; CI anterior não a qualifica automaticamente.

**Estado histórico confirmado na rodada de infraestrutura:** no [CI de `ec4b7f2`, run 39681738837](https://github.com/cybermanju/cybermanju-shell/actions/runs/39681738837), passaram os seis jobs de ELF, kbuild, staging host, type-check, regressões e runtime de módulos. O job gráfico falhou **antes de executar qualquer VM**, no `apt-get update`: o repositório Microsoft não relacionado ao teste devolveu HTTP 403. Não foi falha de compilação ou readback. Os artefatos nativos remotos foram baixados em `artifacts/ci-ec4b7f2/`: incluem reserva, os sete cenários brutos e o readback preparado temporário de uma CPU, com `init_called=false`.

A correção da rodada de infraestrutura trocou somente o setup dos três jobs que instalam ferramentas pelo [helper APT isolado](../tools/ci/install_runtime_tools.sh). Usa índices/cache temporários, fontes Ubuntu oficiais, keyring Ubuntu explícito e erro estrito no update, sem modificar as fontes globais, aceitar pacotes sem assinatura ou pular VMs. Mantém os sete jobs, versões/pins, budgets e gates. As sete regressões novas cobrem os três perfis, falhas de update/install, cleanup, fonte Microsoft preexistente preservada, input inválido e `os-release` tratado como dados.

Passaram **130 testes Python e os 2 ktests gráficos** (contrato e MMIO), com os seis limites de escrita preservados. O APT real atualizou somente índices Ubuntu assinados e resolveu em **simulação** o perfil completo `module-staging`, inclusive headers `6.8.0-147.147`, GCC 13 e kmod; não instalou pacotes. Log em `artifacts/cycle-1900/apt-module-staging-simulation.log`. Essa verificação não substitui a próxima execução hospedada do CI e não é teste de instalação da ISO. Código do loader e a lista de módulos qualificados não mudaram.

O [run ISO de `540a0b8`, 39666736082](https://github.com/cybermanju/cybermanju-shell/actions/runs/39666736082), terminou em timeout de 3.600 segundos esperando o marcador exato de instalação concluída. O console ainda copiava paths Nix; não houve boot instalado nesse run. Logs e métricas estão em `artifacts/iso-diagnosis-1900/`. A ISO de `9b31616` iniciou depois; não foi cancelada nesta rodada. Nenhum timeout ou gate de sucesso foi relaxado.

Na referência `fb5b9d0`, o [CI de regressões 39658860905](https://github.com/cybermanju/cybermanju-shell/actions/runs/39658860905) passou. O [CI de ISO 39658861099](https://github.com/cybermanju/cybermanju-shell/actions/runs/39658861099) superou preparação, build, metadados e live USB, mas **falhou na espera de sucesso da instalação UEFI após 3.600 segundos em TCG**. O console ainda mostrava cópia de paths Nix; boot instalado foi pulado. Diagnósticos baixados em `artifacts/iso-diagnosis-1730/`; não é prova de falha do kernel instalado nem motivo para afrouxar o marcador de sucesso. Não foi cancelado nem conta como aceite completo. A referência `540a0b8` teve o [CI de regressões 39666735817](https://github.com/cybermanju/cybermanju-shell/actions/runs/39666735817) aprovado, incluindo a reserva; seu [run ISO 39666736082](https://github.com/cybermanju/cybermanju-shell/actions/runs/39666736082) estava em progresso naquela consulta; o desfecho está registrado acima. A referência `93aafd8` teve o [CI de regressões 39672096994](https://github.com/cybermanju/cybermanju-shell/actions/runs/39672096994) aprovado. Na consulta desta etapa, a ISO de `540a0b8` continua no gate de instalação/verificação EFI GRUB, preservada. O [CI de regressões de `9b31616`, run 39676687421](https://github.com/cybermanju/cybermanju-shell/actions/runs/39676687421), também passou. Na consulta desta rodada, sua ISO estava pendente e a de `540a0b8` seguia em instalação UEFI, sem cancelamento por esta execução. O protótipo temporário foi validado localmente; CI anterior não é aprovação da nova revisão.

A base `6b7b043` teve o [CI 39647158355](https://github.com/cybermanju/cybermanju-shell/actions/runs/39647158355) aprovado nos seis jobs, incluindo o runtime bruto. A nova cópia de ELF em frames foi testada localmente e acrescentada ao mesmo job, sem transferir automaticamente a aprovação anterior. A consulta confirmou main sincronizada, nenhuma PR aberta e branch histórica incorporada. O [CI ISO 39636467347](https://github.com/cybermanju/cybermanju-shell/actions/runs/39636467347), revisão `8643c76`, continuava na instalação UEFI; foi preservado. O run ISO de `6b7b043` estava pendente, sem nova evidência de boot instalado nesta rodada.

**Segundo run completo aprovado:** o [CI ISO 39623025465](https://github.com/cybermanju/cybermanju-shell/actions/runs/39623025465), revisão `47ee9b5`, concluiu com sucesso live USB, instalação/verificação EFI GRUB e boot instalado. O `boot.log` baixado contém `Test case 'hello' passed` e `PASS: installed Asterinas booted and hello-asterinas passed (tcg)`. Logs em `artifacts/iso-39623025465/`; não foi preciso baixar a ISO inteira. É outra revisão anterior, QEMU x86-64/UEFI/TCG, não prova do staging ou de `.ko`.

O [run ISO 39608968083](https://github.com/cybermanju/cybermanju-shell/actions/runs/39608968083), base `3d6dc25`, comprovou live USB, instalação UEFI CD/EFI GRUB e boot instalado. `boot.log` registra `PASS: installed Asterinas booted and hello-asterinas passed (tcg)` e **1 passou / 0 falhou**; serial registra EFI/OSTD. Evidência local em `artifacts/iso-39608968083/`. Foi QEMU/TCG, não hardware físico nem teste de módulos. O run antigo `39605334894` falhou na prontidão serial antes desses gates, sem causa isolada.

A ISO mantém `cancel-in-progress: false`; nenhum agendamento foi criado ou alterado nesta rodada.
