# Módulos Linux: o que funciona e o que falta provar

**Nenhum módulo Linux `.ko` foi demonstrado carregando neste kernel.** A biblioteca Rust valida parte da estrutura ELF e dos símbolos; não é um loader nem uma promessa de compatibilidade com drivers Linux.

## Matriz atual

| Componente / alvo | Versão ou formato | Evidência mais recente | Limite |
|---|---|---|---|
| Fonte Asterinas do fork | `0.19.0`, conforme `asterinas/VERSION` | Fonte local e `main` remota conferidas na base `6916527` | Kernel completo não recompilado nem inicializado nesta rodada |
| Toolchain do kernel | `nightly-2026-07-21`, conforme `rust-toolchain.toml` | Pin lido no repositório | Testes no host não substituem build com esse nightly |
| Pré-validador `linux-module-elf` | ELF64 little-endian, ET_REL, x86-64 | **26 testes Rust** aprovados no host Linux x86-64, `rustc 1.97.1`; build `no_std`, sem dependências e sem `unsafe` aprovado | Não integrado a syscalls ou ao boot; não resolve símbolos nem aplica relocations |
| Tabelas de símbolos | `SHT_SYMTAB`, entradas ELF64 de 24 bytes | Testes de `.strtab`, nomes, partição local/global, índices de seção, overflow e limites de símbolos aprovados | `SHT_DYNSYM` e índices estendidos rejeitados; símbolos indefinidos, absolutos e common não são resolvidos |
| Objeto C real de teste | GCC 13.3.0, ELF ET_REL x86-64 com 14 seções | Compilação e inspeção, incluindo símbolos, aprovadas; `readelf` confirmou `.bss`, `.modinfo` e relocations | Objeto C comum, **não** `.ko` produzido por Linux kbuild; nenhuma relocação aplicada ou entrada executada |
| ARM64 / RISC-V / LoongArch | Nenhuma versão de módulo qualificada | Arquitetura diferente rejeitada pelo pré-validador; exemplo sintético ARM64 | Nenhuma execução de módulo nessas arquiteturas |
| Módulos Linux reais | Nenhum nome/versão aprovado | Nenhuma evidência de `.ko` carregado | Kernel/configuração Linux alvo, `Module.symvers`, `vermagic` e assinaturas ainda sem qualificação |
| Ponte NVIDIA NVport | Fonte NVIDIA `615.78.08`, pin `1a82fbfda8f321b056eb3568850f9316c5e465c5` documentado na biblioteca | Documentação local, sem nova execução NVport nesta rodada | Ponte por fontes C/Rust, não carregamento de `nvidia.ko`; RM completo, GSP e GPU real não validados aqui |
| CI de regressão na base `6916527` | [Run 39600721229](https://github.com/cybermanju/cybermanju-shell/actions/runs/39600721229) | Jobs `regression` e `linux-module-elf` aprovados | Esse resultado cobre os 14 testes ELF anteriores, não automaticamente as alterações posteriores |
| ISO / live USB Linux | [Run 39600721252](https://github.com/cybermanju/cybermanju-shell/actions/runs/39600721252), QEMU TCG / UEFI | ISO construída/verificada; métricas readiness, live-checks e shutdown com `success`; serial registra `reboot: Power down` | Job falhou no gate shell posterior, antes de instalação e boot instalado; não constitui aceitação completa da ISO |
| Instalação e boot Asterinas instalado | Mesmo run | Etapas puladas após a falha do gate USB | Não validados; não confundir boot do instalador Linux com boot Asterinas |

## Melhoria desta rodada

A biblioteca em [`asterinas/kernel/libs/linux-module-elf`](../asterinas/kernel/libs/linux-module-elf/src/lib.rs) segue `no_std`, sem alocação, dependências externas ou `unsafe`. Além dos checks de cabeçalho/seções/nomes, o módulo [`symbols.rs`](../asterinas/kernel/libs/linux-module-elf/src/symbols.rs) valida tamanho e divisibilidade da tabela de símbolos, `sh_link` apontando para string table, terminadores e limites dos nomes, símbolo nulo, `sh_info` e ordenação de símbolos locais, índices de seção e `st_value + st_size` sem overflow dentro da seção de destino. `.bss` usa tamanho em memória, não payload no arquivo. Símbolos indefinidos, absolutos e common passam apenas pela validação estrutural aplicável: não há resolução.

Foram acrescentados 12 testes, inclusive truncamentos e mutações da amostra com símbolos. Esses testes finitos **não** equivalem a fuzzing exaustivo ou auditoria formal.

O pré-validador não verifica ainda relocations, semântica completa das flags/tipos de símbolos, sobreposições de regiões, ABI Linux ou autenticidade. Um retorno positivo **nunca** deve autorizar mapeamento executável ou chamada de entrada de módulo.

## Bloqueio de ISO identificado

O artefato do run `39600721252` contém `boot-metrics-live-usb-iso-boot-readiness.json`, `boot-metrics-live-usb-live-checks.json` e `boot-metrics-live-usb-shutdown.json`, todos com `status: success`. O marcador real no `qemu.log` vem imediatamente após metadados OSC do systemd (`ESC ] 3008;... ESC \\`). O harness Python normaliza esses controles e passou; o `grep` shell ancorado sobre o log bruto não os normalizava e devolveu erro. Portanto não há evidência de falha do shutdown nesse run; há um falso negativo posterior à VM. A mensagem de udev durante o encerramento não é, isoladamente, causa-raiz.

A correção remove essa dupla checagem inconsistente, mantendo obrigatórios o sucesso do harness Python, seus checks reais de marcador/comandos/shutdown e a imagem de disco na fase de instalação. Precisa de novo CI completo para provar instalação e boot instalado.

## Testes reproduzíveis

Na raiz do repositório:

```bash
bash tools/ci/test_linux_module_elf.sh
python3 -m unittest discover -s tools/ci -p 'test_*.py' -v
python3 -m unittest discover -s gpui-shell/scripts -p 'test_frame_telemetry.py' -v
```

Resultados locais: **26 testes Rust**, inspeção do objeto C real, **70 regressões CI/ISO** e **19 regressões de telemetria** aprovados. Os testes antigos do gate foram preservados, com o stub ajustado para reproduzir o contrato real do harness ao faltar marcador; quatro testes adicionais cobrem OSC, falha do harness e exigência de imagem instalada. `bash -n tools/ci/*.sh` e `git diff --check` também passaram. Os logs desta rodada ficam em `artifacts/linux-module-symbols/` e os artefatos remotos em `artifacts/iso-39600721252/`, ambos ignorados pelo Git. O CI de regressão da base já foi confirmado; resultados das novas alterações precisam ser conferidos por commit, sem herdar aprovação de revisão anterior.

## Próxima pequena etapa

Validar estrutura e referências de `.rela.*` x86-64: `sh_link`, `sh_info`, tamanhos das entradas, índices de símbolos e intervalos do destino, com testes negativos, ainda sem aplicar relocations. Depois fixar um kernel Linux/configuração e compilar um módulo mínimo real via kbuild, mantendo separadas inspeção, resolução, relocation e execução.

Antes da execução serão necessários política de confiança/assinatura, privilégio, W^X, ciclo de vida e rollback. Módulos Linux podem executar operações arbitrárias em ring 0: a fronteira de confiança precisa ficar explícita em OSTD, sem introduzir `unsafe` no kernel seguro. Aprovar um módulo mínimo não aprova todos os drivers ou versões do Linux.
