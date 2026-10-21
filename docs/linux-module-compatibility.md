# Módulos Linux: o que funciona e o que falta provar

**O módulo Linux mínimo passou pela nova operação atômica por alvo: 13 relocations em 14 cópias PROGBITS/NOBITS, incluindo alvos sem relocations. Os exports são sintéticos; nenhum `.ko` foi carregado ou executado.** Separadamente, o CI comprovou instalação e boot do Asterinas instalado com um teste `hello` aprovado, em QEMU x86-64/UEFI com TCG, não em hardware físico.

## Matriz atual

| Componente / alvo | Versão ou formato | Evidência mais recente | Limite |
|---|---|---|---|
| Fonte Asterinas do fork | `0.19.0`, conforme `asterinas/VERSION` | Fonte local e `main` conferidas na base `47ee9b5` | Boot remoto comprovado somente na revisão da ISO abaixo; sem boot local nesta rodada |
| Toolchain do kernel | `nightly-2026-07-21`, pin do repositório | Pin preservado | Testes no host não substituem build com esse nightly |
| Biblioteca `linux-module-elf` | ELF64 little-endian ET_REL, x86-64 | **82 testes Rust**, `rustc 1.97.1`, `no_std`, sem dependências externas ou `unsafe`; etapas offline usam `alloc` | Não integrada a syscalls ou loader executável |
| Plano de seções alocáveis | PROGBITS, NOTE e NOBITS com `SHF_ALLOC`, sem TLS | `.ko`: **16 seções / span 1.856 bytes**; objeto C: **6 / 320 bytes** | Metadados e endereços numéricos, sem mapear/reservar memória do módulo |
| Resolução + RELA usando o plano | Símbolos definidos, ABS e exports explicitamente fornecidos | `.ko`: **13 escritas em 9 cópias ALLOC**, com **3 rejeições de export ausente**; C: **4 escritas em 2 cópias**, com 1 rejeição | Exports e base sintéticos; sem funções reais do kernel ou validação de ABI |
| Operação atômica por alvo | Todas as RELA de um PROGBITS/NOBITS planejado | `.ko`: **13 escritas / 14 cópias completas**; C: **4 / 5**; testes sintéticos com duas tabelas no mesmo alvo | Atomicidade por seção, não por módulo; duas tabelas são fixture sintética, não característica comprovada deste `.ko` |
| Aplicação RELA anterior, independente | Inclui seções debug e endereços sintéticos separados | `.ko`: **2.909 escritas em 14 cópias**; C: **4 em 2**; regressão preservada | Esses 2.909 casos não são aplicação do novo layout nem prova de execução |
| **Módulo Linux real** | **`cybermanju_fixture.ko` `0.1.0`, Linux `6.8.0-147-generic`, x86-64** | **kbuild/MODPOST/link, inspeção de 44 seções e testes offline acima aprovados** | **Nunca carregado; nenhum contrato de execução Linux/Asterinas comprovado** |
| Outros módulos/versões Linux | Nenhum qualificado | Nenhuma evidência de carregamento, inclusive do módulo mínimo | Não extrapolar para GPU, outras configurações ou versões Linux |
| ARM64 / RISC-V / LoongArch | Nenhum módulo qualificado | Pré-validador limitado a x86-64; exemplo sintético ARM64 rejeitado | Nenhum build/execução de `.ko` nessas arquiteturas |
| NVIDIA NVport | Fonte `615.78.08`, pin `1a82fbfda8f321b056eb3568850f9316c5e465c5` documentado localmente | Sem nova execução NVport nesta rodada | Ponte C/Rust por fontes, não `nvidia.ko`; RM completo, GSP e GPU real não validados aqui |
| CI da base `47ee9b5` | [Run 39623025079](https://github.com/cybermanju/cybermanju-shell/actions/runs/39623025079) | Jobs de regressão, ELF e kbuild aprovados, incluindo 71 testes Rust anteriores | Não transfere aprovação automaticamente para os novos 82 testes |
| ISO live USB, QEMU/UEFI | [Run 39608968083](https://github.com/cybermanju/cybermanju-shell/actions/runs/39608968083), base `3d6dc25` | Gate live USB concluído com sucesso | Boot do instalador Linux, não do Asterinas instalado; não é teste em hardware USB físico |
| Instalação via UEFI CD / EFI GRUB | Mesmo run | Gate **concluído com sucesso** nesta consulta | Não prova que o kernel instalado chega ao userspace |
| Boot Asterinas instalado / hello userspace | Mesmo run, **concluído com sucesso** | Entrada EFI/OSTD no serial; `hello`: **1 passou / 0 falhou**, marcador `boot.passed` | QEMU x86-64/UEFI, TCG; não qualifica desktop, GPU, hardware ou módulos `.ko` |

## O que mudou na resolução

A API [`SymbolResolver`](../asterinas/kernel/libs/linux-module-elf/src/resolve.rs) valida o ELF e recalcula o plano recebido antes de resolver símbolos pedidos pelo chamador. A igualdade do plano é **estrutural**, não uma identidade criptográfica do arquivo. Os bytes e o plano são emprestados imutavelmente durante o uso.

Símbolos definidos usam o endereço da seção planejada mais `st_value`, com adição verificada. ABS usa seu valor absoluto. Símbolos em seções não planejadas são rejeitados, sem override por export. COMMON não recebe alocação automática e é recusado.

Undefined global/weak exige um nome exato numa lista de `Export { name, value }` fornecida explicitamente. Weak ausente também é erro, não zero implícito; zero explicitamente fornecido é aceito. Undefined local, visibilidade/bits `st_other` não padrão, símbolo nulo e tipos não admitidos são recusados. A lista aceita apenas bindings local/global/weak e tipos NOTYPE/OBJECT/FUNC/SECTION; TLS, IFUNC e FILE não são resolvidos por esta API.

Os exports são copiados com reserva falível, ordenados e buscados por nome em bytes, sem exigir UTF-8. Há teto de **4.096 exports**; nomes vazios/com NUL e nomes duplicados são rejeitados, mesmo com valores iguais. Isso não implementa descoberta de exports do kernel, política GPL, CRC/modversions ou prova de ABI.

O probe usa valores de teste fixos para `unresolved_kernel_symbol`, `__fentry__` e `__x86_return_thunk`. Confere a resolução e cada campo RELA escrito contra um cálculo independente, verifica rejeição sem exports e preserva o ELF original. **Esses números não apontam para implementações dessas funções.** Seções debug/não ALLOC ficam fora do teste integrado; seu teste offline anterior permanece separado.

A nova API `SymbolResolver::relocate_target` usa os mesmos bytes/layout imutáveis do resolvedor: o chamador não pode fornecer outro ELF ou endereço. Conta todas as entradas das tabelas que apontam para o alvo, inclusive NONE, antes de reservar patches/payload; respeita o orçamento do chamador e o teto agregado de 65.536 entradas. Cada tabela usa sua própria symtab. Valida símbolos, aritmética e sobreposição também entre tabelas antes de devolver uma única cópia completa; erro não devolve resultado parcial. Sem RELA, copia PROGBITS ou zera NOBITS. NOTE/não ALLOC são recusados como alvos. O probe compara todos os bytes, incluindo áreas intactas. A API anterior `relocate_section` permanece disponível e compartilha o encoder de larguras/overflow.

## A amostra real e sua procedência

O [fixture C](../asterinas/kernel/libs/linux-module-elf/tests/kbuild/cybermanju_fixture.c) tem init que retorna zero, exit vazio e metadados. Foi produzido por Linux kbuild real, não por renomear um objeto C. O [executor](../tools/ci/test_linux_module_kbuild.sh) faz build, `readelf`, `modinfo`, `nm` e testes Rust; não usa comandos de carregamento.

A configuração testada é Linux **`6.8.0-147-generic` x86-64**, headers comuns/genéricos Ubuntu **`6.8.0-147.147`**, compilador **`x86_64-linux-gnu-gcc-13` `13.3.0-6ubuntu2~24.04.1`**, inspetor Rust **`1.97.1`**. `vermagic`: **`6.8.0-147-generic SMP preempt mod_unload modversions`**. Os tipos encontrados são `R_X86_64_32`, `R_X86_64_64`, `R_X86_64_PC32` e `R_X86_64_PLT32`.

`__fentry__` e `__x86_return_thunk` continuam indefinidos no ELF: o teste apenas fornece valores numéricos para eles. **BTF não foi gerado**, por falta de `vmlinux` correspondente. Assinatura/confiança, CRCs e execução não foram qualificados.

| Artefato local | SHA-256 |
|---|---|
| `cybermanju_fixture.ko` | `84170594a81fa2c6913e252a749934d282a167400234de44305f9b41a8bfd4ab` |
| Fonte C | `ae0db544d38a724cdac33d74b0d54716628dd23d87bc613dae2f072dd72e3da7` |
| `.config` dos headers | `cf8454bc2e67effab87a5bff3c39042f2d8d106b6a777c4302e52f50d23b7400` |
| `Module.symvers` | `8e7eb5798789b9610c324b3d9b13373d88c30ae17bb9e41e622efd05e0277a87` |

O `.ko`, metadados, `inspection.txt`, `provenance.txt` e `status.txt` ficam em `artifacts/linux-module-kbuild/`, ignorados pelo Git. O status desta execução é `exit_code=0`, `loaded=false`; falhas de rerun não devem herdar um provenance antigo. O build não promete binário idêntico entre diretórios/toolchains: cada amostra é identificada pelo próprio hash. Os logs desta rodada estão em `artifacts/linux-module-atomic/`.

## Limites que permanecem

O planner usa índices, alinhamentos, padding, limites de bytes/contagem e overflow verificados; NOTE é dado opaco, TLS é recusado. Não cria mapeamentos nem aplica permissões. `inspect` permanece estrutural e não verifica a ABI Linux, autenticidade ou semântica completa de símbolos/seções.

`relocate_section` aplica `S + A` ou `S + A - P` em `i128` e exige representação exata em `u64`, `i64`, `u32` ou `i32`. Retorna uma nova cópia de dados de PROGBITS/NOBITS, sem solicitar permissão executável, sem transformar números em ponteiros e sem modificar o ELF. Recusa escritas sobrepostas na tabela, símbolo sem valor e excesso de orçamento/65.536 entradas. RELA reconhece `NONE`, `64`, `PC64`, `PC32`, `PLT32`, `32` e `32S`; REL/RELR e outros tipos são recusados. PLT32 usa o destino direto fornecido, sem construir PLT.

Ainda faltam transação conjunta de todas as seções do módulo com orçamento agregado, exports reais, ABI/modversions, confiança/assinatura, privilégio, W^X, ciclo de vida e rollback. Módulos executam operações arbitrárias em ring 0: a fronteira de confiança terá de ficar explícita em OSTD, sem introduzir `unsafe` no kernel seguro. Aprovar este fixture não aprova outros drivers ou versões Linux. Testes finitos de truncamento/mutação não equivalem a fuzzing exaustivo ou auditoria formal.

## CI, boot e reprodução

O run antigo [39605334894](https://github.com/cybermanju/cybermanju-shell/actions/runs/39605334894) falhou na prontidão serial: timeout de `/dev/ttyS0` e ausência do marcador, antes de live-checks/shutdown. A causa da ausência do dispositivo não foi isolada. O run `39608968083` agora terminou com sucesso: `boot.log` registra `PASS: installed Asterinas booted and hello-asterinas passed (tcg)` e `Passed: 1 / Failed: 0`; `qemu-serial.log` registra entrada EFI e OSTD. As métricas de instalação e shutdown também são success. Evidências locais em `artifacts/iso-39608968083/`. Foi emulação TCG com KVM indisponível/desativado, não teste físico. Não confundir esse boot com carregamento de módulos, nem declarar estabilidade a partir de um único sucesso.

A ISO mantém `cancel-in-progress: false`: pushes periódicos não devem interromper o run ativo, embora a pendente mais recente possa substituir outra pendente. Nenhum agendamento foi criado/alterado nesta rodada.

```bash
bash tools/ci/test_linux_module_elf.sh
bash tools/ci/test_linux_module_kbuild.sh artifacts/linux-module-kbuild
python3 -m unittest discover -s tools/ci -p 'test_*.py' -v
python3 -m unittest discover -s gpui-shell/scripts -p 'test_frame_telemetry.py' -v
```

Resultados locais: **82 testes Rust**, as duas fixtures com aplicação atômica por alvo acima, **76 regressões CI/ISO** e **19 de telemetria** aprovados. Onze testes novos cobrem duas tabelas no mesmo alvo, overlap entre tabelas, orçamento agregado/NONE/teto rígido, erro de símbolo e overflow tardios sem mutação, NOBITS, alvo sem RELA, alvos recusados, tamanho, symtabs diferentes com valores distintos e truncamentos/mutações pela API pública sem panic. `bash -n tools/ci/*.sh` e `git diff --check` passaram. Os jobs ELF/kbuild existentes executam o probe ampliado; seu novo CI deve ser conferido pelo commit publicado.

## Próxima pequena etapa

Preparar todas as seções de dados do módulo como uma unidade, com orçamento total de payload/entradas/seções e retorno completo ou erro se uma seção posterior falhar. Manter NOTE opaco, decidir explicitamente sua política e testar que falhas não expõem um módulo parcialmente preparado. Não chamar qualquer função do `.ko` antes de projetar e testar as fronteiras de execução e confiança.
