# Módulos Linux: o que funciona e o que falta provar

**Nenhum módulo Linux `.ko` foi demonstrado carregando neste kernel.** A biblioteca Rust faz pré-validação estrutural de ELF, símbolos e um subconjunto de relocations x86-64. Não resolve símbolos, não aplica relocations e não executa código de módulo.

## Matriz atual

| Componente / alvo | Versão ou formato | Evidência mais recente | Limite |
|---|---|---|---|
| Fonte Asterinas do fork | `0.19.0`, conforme `asterinas/VERSION` | Fonte local e `main` remota conferidas na base `9d13c04` | Kernel completo não recompilado nem inicializado nesta rodada |
| Toolchain do kernel | `nightly-2026-07-21`, conforme `rust-toolchain.toml` | Pin do repositório | Testes no host não substituem build com esse nightly |
| Pré-validador `linux-module-elf` | ELF64 little-endian, ET_REL, x86-64 | **40 testes Rust** aprovados no host Linux x86-64, `rustc 1.97.1`; build `no_std`, sem dependências e sem `unsafe` aprovado | Biblioteca ainda não integrada a syscalls, boot ou loader executável |
| Símbolos | `SHT_SYMTAB`, entradas ELF64 de 24 bytes | Nomes, string table, partição local/global, índices e limites de símbolos validados | `SHT_DYNSYM` e índices estendidos rejeitados; símbolos indefinidos, absolutos e common não são resolvidos |
| Relocations | `SHT_RELA`, entradas de 24 bytes | `sh_link`, `sh_info`, índices de símbolos, tipo e intervalo do destino validados com testes negativos | Somente os tipos listados abaixo; nenhuma fórmula calculada, escrita em memória ou execução |
| Objeto C real de teste | GCC 13.3.0, ELF ET_REL x86-64 com 14 seções | Compilação, `readelf` e inspeção com símbolos e RELA aprovados | Objeto C comum, **não** `.ko` de Linux kbuild; não prova ABI Linux ou módulo executável |
| ARM64 / RISC-V / LoongArch | Nenhuma versão de módulo qualificada | Arquitetura diferente rejeitada pelo pré-validador; exemplo sintético ARM64 | Nenhuma execução de módulo nessas arquiteturas |
| Módulos Linux reais | Nenhum nome/versão aprovado | Nenhuma evidência de `.ko` carregado | Kernel/configuração Linux alvo, `Module.symvers`, `vermagic` e assinaturas ainda sem qualificação |
| Ponte NVIDIA NVport | Fonte NVIDIA `615.78.08`, pin `1a82fbfda8f321b056eb3568850f9316c5e465c5` documentado localmente | Sem nova execução NVport nesta rodada | Ponte C/Rust por fontes, não carregamento de `nvidia.ko`; RM completo, GSP e GPU real não validados aqui |
| CI de regressão na base `9d13c04` | [Run 39605334934](https://github.com/cybermanju/cybermanju-shell/actions/runs/39605334934) | Jobs de regressão e ELF aprovados | Cobre os 26 testes Rust anteriores, não automaticamente os novos 40 |
| ISO / instalação / boot instalado | [Run 39605334894](https://github.com/cybermanju/cybermanju-shell/actions/runs/39605334894), base `9d13c04` | Run consultado: em andamento no gate live USB | Instalação e boot instalado ainda pendentes nessa consulta; boot do instalador Linux não é boot Asterinas |

## Melhoria desta rodada

O novo [`relocations.rs`](../asterinas/kernel/libs/linux-module-elf/src/relocations.rs) roda depois da validação das seções e símbolos. Confere tamanho/divisibilidade da tabela RELA, `sh_link` para uma symtab validada, `sh_info` para seção de destino não nula, índice de símbolo em `r_info` e `r_offset + largura` sem overflow dentro do tamanho da seção. `.bss`/`SHT_NOBITS` usa tamanho em memória, não bytes no arquivo. Tabelas vazias continuam exigindo referências válidas.

A lista de tipos reconhecidos é deliberadamente pequena:

| Tipo x86-64 | Largura conferida | O que foi provado |
|---|---:|---|
| `R_X86_64_NONE` (0) | 0 bytes | Política conservadora exige offset até o fim da seção, mesmo sem escrita |
| `R_X86_64_64` (1), `R_X86_64_PC64` (24) | 8 bytes | Campo cabe no destino; não há cálculo da relocação |
| `R_X86_64_PC32` (2), `R_X86_64_PLT32` (4), `R_X86_64_32` (10), `R_X86_64_32S` (11) | 4 bytes | Campo cabe no destino; não há resolução de símbolo, cálculo ou checagem de overflow do resultado da fórmula |

Tipos fora dessa lista e tabelas `SHT_REL`/`SHT_RELR` são rejeitados. Reconhecer largura **não significa implementar ou suportar a relocação num módulo executável**. Addends negativos/extremos são lidos como parte do contêiner, mas não avaliados. O teste de imutabilidade confirma que a inspeção não altera a entrada.

Foram adicionados **14 testes Rust**: limites exatos, overflow, símbolo/links/destinos inválidos, entradas parciais, tabelas vazias, tipos não implementados, `.bss`, imutabilidade, truncamentos e mutações. São testes finitos, não fuzzing exaustivo ou auditoria formal. Sobreposições de regiões/escritas, semântica completa de símbolos/seções, ABI Linux, assinaturas e execução seguem fora do escopo. Um resultado positivo **nunca** autoriza mapear ou chamar código.

## CI longo sem reinícios a cada push

A política de concorrência da ISO passou para `cancel-in-progress: false`, mantendo o grupo por branch/ref. Assim, novas alterações não devem interromper um run ativo antes da instalação e do boot. O GitHub ainda pode substituir uma execução **pendente** pela pendente mais recente; isso não transforma jobs cancelados ou resultados de outra revisão em aprovação.

O run anterior `39600721252` havia produzido métricas de readiness/live-checks/shutdown bem-sucedidas, mas falhou no `grep` shell sobre marcador com metadados OSC. Essa checagem duplicada foi removida em `9d13c04`, preservando os checks reais do harness Python. A validação completa dessa correção continua dependendo do resultado do run de ISO acima.

## Testes reproduzíveis

Na raiz do repositório:

```bash
bash tools/ci/test_linux_module_elf.sh
python3 -m unittest discover -s tools/ci -p 'test_*.py' -v
python3 -m unittest discover -s gpui-shell/scripts -p 'test_frame_telemetry.py' -v
```

Resultados locais desta rodada: **40 testes Rust**, inspeção do objeto C real, **72 regressões CI/ISO** e **19 regressões de telemetria** aprovados. Incluem duas regressões da política de concorrência, sem remover os testes existentes. `bash -n tools/ci/*.sh` e `git diff --check` também passaram. Logs locais em `artifacts/linux-module-relocations/`, ignorados pelo Git. Nenhuma nova evidência local de boot, instalação ou GPU.

## Próxima pequena etapa

Fixar versão/configuração de Linux e construir um `.ko` mínimo real via kbuild; submetê-lo primeiro à inspeção, registrando hash, toolchain, `vermagic` e tipos de relocations encontrados. Isso será evidência de **inspeção de módulo real**, não de carregamento. Depois implementar, em memória não executável, uma etapa de planejamento/aplicação de relocations com testes de overflow e símbolos não resolvidos antes de integrar ao kernel.

Antes da execução serão necessários política de confiança/assinatura, privilégio, W^X, ciclo de vida e rollback. Módulos Linux executam operações arbitrárias em ring 0: a fronteira de confiança precisa ficar explícita em OSTD, sem introduzir `unsafe` no kernel seguro. Aprovar um módulo mínimo não aprova todos os drivers ou versões do Linux.
