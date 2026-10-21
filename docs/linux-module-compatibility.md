# Módulos Linux: o que funciona e o que falta provar

**Uma segunda fixture kbuild real foi preparada com zero exports: 13 seções, 1.721 bytes e 6 entradas RELA. `nm -u` ficou vazio e init/exit foram localizados, mas não chamados.** Ela usa flags de teste não padrão e desvia o objtool somente nesse objeto; isso não valida mitigação ou segurança. A fixture original foi preservada e ainda usa exports sintéticos. Nenhum `.ko` foi mapeado, carregado ou executado. A meta continua demonstrar uma fixture real em QEMU/Asterinas, sem prometer compatibilidade universal.

Separadamente, o CI já comprovou instalação e boot do Asterinas instalado com `hello` aprovado, em QEMU x86-64/UEFI com TCG. Isso não constitui evidência de execução de módulos nem teste em hardware físico.

## Matriz atual

| Componente / alvo | Evidência | Limite |
|---|---|---|
| Asterinas do fork | `0.19.0`; base desta implementação `57cb8c9` | Nenhum novo boot nesta rodada |
| Toolchain do kernel | Pin `nightly-2026-07-21` preservado | Teste host não substitui build do kernel |
| Biblioteca `linux-module-elf` | **96 testes Rust**, `rustc 1.97.1`, `no_std`, sem dependências externas; `forbid(unsafe_code)` | Não integrada a syscalls ou loader executável |
| Plano SHF_ALLOC | `.ko`: **16 seções / span 1.856 bytes**; C: **6 / 320 bytes** | Metadados e endereços numéricos; nenhuma reserva de memória executável |
| Resolução + RELA | `.ko`: **13 escritas em 9 cópias ALLOC**, 3 rejeições de export ausente; C: **4 em 2**, 1 rejeição | Exports sintéticos, nenhuma prova de ABI |
| Operação por alvo | `.ko`: **13 escritas / 14 cópias PROGBITS/NOBITS**; C: **4 / 5** | API anterior preservada; NOTE não é alvo permitido |
| **Transação do módulo completo** | `.ko`: **16 seções, 1.794 bytes, 13 entradas**; C: **6, 310 bytes, 4 entradas**; comparação de todos os bytes com oracle | Dados de heap, não imagem executável; NOTE copiado opaco |
| RELA independente anterior | `.ko`: **2.909 escritas em 14 cópias**, incluindo debug; C: **4 em 2** | Não confundir esse total com as 13 entradas ALLOC |
| **`cybermanju_fixture.ko` real** | kbuild/MODPOST/link Linux **`6.8.0-147-generic` x86-64**, inspeção de 44 seções e testes acima | **Nunca carregado; ABI Linux/Asterinas não demonstrada** |
| **`cybermanju_noimports_fixture.ko` real, auxiliar** | Mesmo Linux/arquitetura; **0 imports / 0 exports fornecidos**, preparação de **13 seções / 1.721 bytes / 6 RELA**; init FUNC 4 bytes e exit FUNC 2 bytes | Flags não padrão e bypass de objtool restritos ao objeto; sem chamada, mapping, W^X real ou prova de ABI/segurança |
| Outros drivers/versões/arquiteturas | Nenhum módulo qualificado | Sem promessa para GPU, NVIDIA, ARM64, RISC-V ou LoongArch |
| NVIDIA NVport | Fonte `615.78.08`, pin `1a82fbfda8f321b056eb3568850f9316c5e465c5` previamente documentado | Ponte C/Rust por fontes, não `nvidia.ko`; sem nova validação de RM/GSP/GPU |

## Fixture sem imports: avanço e desvio explícito

A [segunda fixture](../asterinas/kernel/libs/linux-module-elf/tests/kbuild-noimports/cybermanju_noimports_fixture.c) tem init que retorna zero, exit vazio e `MODULE_INFO(cybermanju_test_only, ...)`. É produzida pelo kbuild, não por renomear um objeto C. Não substitui a fixture original nem altera flags de kernel, drivers ou testes existentes.

Seu [Makefile](../asterinas/kernel/libs/linux-module-elf/tests/kbuild-noimports/Makefile) remove `-pg -mfentry -mfunction-return=thunk-extern` e acrescenta `-fno-instrument-functions -mfunction-return=keep` **somente para esse objeto**. `OBJECT_FILES_NON_STANDARD_... := y` desvia os checks objtool nesse objeto: sem o desvio, o objtool recusou esse perfil de retorno; sem remover o thunk, havia import de `__x86_return_thunk`. Portanto, o resultado **não é configuração Linux padrão, prova de mitigação ou autorização para executar**. O vermagic continua identificando os headers de origem, não certificando o perfil alterado.

O [executor](../tools/ci/test_linux_module_noimports.sh) exige headers/release/arquitetura corretos, `nm -u` vazio e preparação Rust com lista de exports vazia. Guarda comando real do compilador, desassemblagem, `readelf`, `modinfo`, hashes e status. O [probe](../asterinas/kernel/libs/linux-module-elf/tests/noimports_probe.rs) rejeita até undefined não referenciados, confere `init_module`/`cleanup_module` globais FUNC em seções marcadas executáveis/não graváveis, e verifica seus limites/endereço numérico. **Flags de seção não são permissões de páginas reais.** Uma mutação torna init um import estruturalmente válido: a preparação deve falhar com `MissingSymbol`, sem inventar um destino. Nenhuma função é chamada. Os endereços do layout continuam sendo um modelo numérico, não ponteiros dos buffers.

## Preparação completa, sem resultado parcial

A nova API [`SymbolResolver::prepare_module`](../asterinas/kernel/libs/linux-module-elf/src/resolve.rs) usa os mesmos bytes e layout imutáveis do resolvedor. Não aceita um segundo ELF, endereços substitutos ou buffers externos para alterar. O construtor já inspeciona o ELF e recalcula o plano; a igualdade do plano é estrutural, não uma identidade criptográfica do arquivo.

Antes de reservar payload ou patches, verifica a soma de bytes de **todas** as seções planejadas, a quantidade de seções (inclusive vazias) e as entradas RELA de **todos** os alvos ALLOC. Entradas NONE contam; o teto rígido é **65.536 entradas no módulo inteiro**, mesmo que cada alvo isolado caiba. Conversões e somas são verificadas. O limite de bytes é de payload, incluindo NOBITS e NOTE; **não** inclui padding de endereço, metadados, capacidade do alocador ou memória temporária de patches. O span com alinhamento continua limitado separadamente por `plan_layout`.

PROGBITS é copiado; NOBITS começa zerado e recebe suas relocations; NOTE é copiado literalmente como metadado opaco. **Qualquer tabela RELA direcionada a NOTE é recusada**, inclusive vazia ou composta apenas por NONE. Tabelas de seções debug/não ALLOC não são aplicadas nem contabilizadas nesse orçamento de entradas, embora sua estrutura continue validada por `inspect`.

Cada alvo usa todas as suas tabelas e a symtab específica de cada tabela. Overlap, símbolo ausente e overflow são erros. Cópias ficam privadas até todas as seções terminarem; se uma seção posterior falhar, as anteriores são descartadas e nenhum `PreparedModule` parcial é devolvido. O ELF e resultados de operações anteriores não mudam. As reservas usam `try_reserve_exact`; falhas retornam `ResourceLimit`.

[`PreparedModule` e `PreparedSection`](../asterinas/kernel/libs/linux-module-elf/src/image.rs) expõem somente getters imutáveis de dados e placements. Os endereços numéricos do layout **não são os endereços dos Vecs**. Não há mapping, solicitação de permissão executável, conversão para ponteiro de função ou chamada de init. Sucesso não concede confiança nem autorização para executar.

O helper privado por alvo reutiliza a validação do resolvedor imutável, sem reinspecionar globalmente todos os símbolos/relocations a cada seção. Ainda percorre cabeçalhos por alvo; os limites de seções do chamador também devem ser conservadores. Os orçamentos não representam uma garantia de tempo de CPU ou de consumo total do alocador.

## Símbolos e relocations: limites preservados

Símbolos definidos usam endereço planejado + `st_value`, com aritmética verificada. ABS usa seu valor; COMMON e definições em seções não planejadas são recusados. Undefined global/weak exige nome exato em exports explicitamente fornecidos. Weak ausente também é erro; zero só é aceito quando fornecido explicitamente. Undefined local, símbolo nulo, visibilidade/bits `st_other` não padrão e tipos/bindings não admitidos são rejeitados.

Há teto de **4.096 exports**, copiados com reserva falível, ordenados e pesquisados por nome em bytes. Nomes vazios, com NUL ou duplicados são recusados. A lista aceita bindings local/global/weak e tipos NOTYPE/OBJECT/FUNC/SECTION; não resolve TLS, IFUNC ou FILE. Isso não implementa descoberta de exports reais, política GPL, CRC/modversions ou ABI.

RELA usa `S + A` ou `S + A - P` em `i128` e exige representação exata no campo destino. Tipos admitidos: NONE, 64, PC64, PC32, PLT32, 32 e 32S. REL/RELR e outros tipos são recusados. PLT32 usa destino direto, sem sintetizar PLT. As APIs anteriores `relocate_section` e `relocate_target` permanecem disponíveis.

## Amostra real e reprodução

O [fixture C](../asterinas/kernel/libs/linux-module-elf/tests/kbuild/cybermanju_fixture.c) tem init que retorna zero, exit vazio e metadados. O [script kbuild](../tools/ci/test_linux_module_kbuild.sh) faz build real, MODPOST, link, `readelf`, `modinfo`, `nm` e o probe Rust. Nunca usa `insmod`/`modprobe` nem executa código do módulo.

Configuração local: headers Ubuntu **`6.8.0-147.147`**, compilador **`x86_64-linux-gnu-gcc-13` `13.3.0-6ubuntu2~24.04.1`**, Rust **`1.97.1`**. Vermagic: `6.8.0-147-generic SMP preempt mod_unload modversions`. Tipos encontrados: R_X86_64_32, R_X86_64_64, R_X86_64_PC32 e R_X86_64_PLT32. BTF não foi gerado por falta de `vmlinux` correspondente.

| Artefato original preservado da base | SHA-256 |
|---|---|
| `cybermanju_fixture.ko` | `6810fe1e122344cef2de3b16437cd1305b786c776cddc7d2e6174f63c75fc68c` |
| Fonte C | `ae0db544d38a724cdac33d74b0d54716628dd23d87bc613dae2f072dd72e3da7` |
| `.config` dos headers | `cf8454bc2e67effab87a5bff3c39042f2d8d106b6a777c4302e52f50d23b7400` |
| `Module.symvers` | `8e7eb5798789b9610c324b3d9b13373d88c30ae17bb9e41e622efd05e0277a87` |

O hash do `.ko` varia entre diretórios de build; cada amostra deve ser identificada por seu próprio hash. Artefatos e provenance estão em `artifacts/linux-module-kbuild/`, ignorados pelo Git; `status.txt`: `exit_code=0`, `loaded=false`. Logs desta rodada em `artifacts/linux-module-whole/`.

Proveniência adicional da fixture sem imports, headers/compilador iguais aos acima:

| Artefato auxiliar local | SHA-256 |
|---|---|
| `cybermanju_noimports_fixture.ko` | `9d81fff4f866fc2497c1ebe99656f56b57a9fe79d37429f033aefce94b362969` |
| Fonte C auxiliar | `abb935fe59de4bd0ac561f4ef5c8ec41ffedc073542a994012cee876cfc605ab` |
| Makefile auxiliar | `b11eaaff663015157e74ff54ed94937208f476475d1b1d87e94cee4b69a0963d` |

Artefatos em `artifacts/linux-module-noimports/`, `status.txt`: `exit_code=0`, `loaded=false`, `undefined_symbols=0` na proveniência. Logs dos testes desta execução em `artifacts/linux-module-noimports-run/`. Os artefatos da fixture original são os preservados da base, não um novo build local nesta rodada.

```bash
bash tools/ci/test_linux_module_elf.sh
bash tools/ci/test_linux_module_kbuild.sh artifacts/linux-module-kbuild
bash tools/ci/test_linux_module_noimports.sh artifacts/linux-module-noimports
python3 -m unittest discover -s tools/ci -p 'test_*.py' -v
python3 -m unittest discover -s gpui-shell/scripts -p 'test_frame_telemetry.py' -v
```

Na base `57cb8c9`, os resultados registrados eram **96 testes Rust, 76 regressões CI/ISO e 19 de telemetria**, objeto C e fixture original completos aprovados, sintaxe/diff sem erros. Os 14 testes adicionados naquela revisão cobrem referência entre seções, bytes intactos, NOBITS/NOTE/vazias, falhas tardias sem resultado parcial, limites agregados e prioridade do preflight, NONE e teto 65.536 em alvos diferentes, NOTE com tabela vazia/NONE, exclusão de debug, plano vazio, padding, 1.024 alvos vazios, RELA debug inválida e truncamentos/mutações sem panic. Testes finitos não equivalem a fuzzing exaustivo nem auditoria formal; o novo CI deve ser conferido pelo SHA publicado.

Nesta rodada passaram novamente **96 testes Rust, 77 regressões CI/ISO e 19 de telemetria**, além do novo probe real sem exports (positivo e import ausente negativo). O novo teste Python verifica quatro falhas antecipadas — headers ausentes, arquitetura, release e symvers — e impede reaproveitar status de sucesso antigo. A fixture original continua no CI; uma etapa e um artefato separados foram acrescentados para o perfil auxiliar. Não somar o cenário do probe à contagem de unit tests Rust.

## O caminho até executar o primeiro `.ko`

A revisão estática não encontrou despacho de `init_module` (175), `delete_module` (176) ou `finit_module` (313) em [`kernel/core/src/syscall/arch/x86.rs`](../asterinas/kernel/core/src/syscall/arch/x86.rs). A biblioteca ainda não é um loader do kernel. Não será suficiente acrescentar uma syscall e chamar os bytes preparados.

O primeiro passo executável deve ser um **protótipo interno controlado em QEMU**, sem entrada arbitrária por syscall: definir confiança na fixture, ABI exata, placement real, chamada de init e critério de sucesso observável no serial. `__fentry__` e `__x86_return_thunk` da fixture original continuam sem implementação compatível. A auxiliar remove esses imports num perfil de teste separado; isso elimina um requisito dessa demonstração específica, mas não implementa exports nem resolve ABI/confiança/W^X. Nunca usar destinos fictícios para executar.

Antes de chamar init, falta uma interface em OSTD para ownership de memória, cópia sem execução, transição para RX/RW sem RWX, sincronização/TLB, descarte e rollback. [`KVirtArea::map_frames`](../asterinas/ostd/src/mm/kspace/kvirt_area.rs) é interno a OSTD e não é uma API pronta de loader com esse contrato. Permissões de VM de processo também não resolvem diretamente memória de kernel. Ciclo de vida, falha de init e política de unload precisam estar explícitos.

Executar código nativo arbitrário em ring 0 não pode virar uma API segura só porque o ELF passou. A fronteira de confiança e qualquer operação de execução de baixo nível pertencem a **OSTD**; `kernel/` e esta biblioteca continuam sem `unsafe`. Permanecem pendentes ABI/modversions, assinatura/confiança e privilégios. A demonstração final será carregar e verificar uma fixture real no Asterinas em QEMU, não certificar drivers arbitrários ou compatibilidade entre versões Linux.

## Evidência de CI e boot já existente

A base `57cb8c9` teve o [run 39629653494](https://github.com/cybermanju/cybermanju-shell/actions/runs/39629653494) concluído com sucesso nos jobs de regressão, ELF e kbuild (96 testes Rust). A ISO ativa `39623025465`, base `47ee9b5`, continua na instalação UEFI e foi preservada. A ISO pendente de `65595af` foi cancelada/substituída pela revisão mais recente, conforme a política de concorrência. Aprovação da base não transfere aprovação automaticamente para esta alteração.

O [run ISO 39608968083](https://github.com/cybermanju/cybermanju-shell/actions/runs/39608968083), base `3d6dc25`, comprovou live USB, instalação UEFI CD/EFI GRUB e boot instalado. `boot.log` registra `PASS: installed Asterinas booted and hello-asterinas passed (tcg)` e **1 passou / 0 falhou**; serial registra EFI/OSTD. Evidência local em `artifacts/iso-39608968083/`. Foi QEMU/TCG, não hardware físico nem teste de módulos. O run antigo `39605334894` falhou na prontidão serial antes desses gates; sua causa não foi isolada.

A ISO mantém `cancel-in-progress: false`; nenhum agendamento foi criado ou alterado nesta rodada.

Próxima pequena etapa: um plano de proteção por páginas que separe código e dados sem compartilhar página gravável/executável, com testes de alinhamento, padding, overflow e conflitos, antes de projetar qualquer mapping ou chamada em OSTD.
