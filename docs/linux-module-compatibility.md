# Módulos Linux: o que funciona e o que falta provar

**Um módulo Linux real foi compilado via kbuild e teve 2.909 relocations aplicadas em 14 cópias de seções mantidas como dados, com endereços sintéticos. Nenhum `.ko` foi carregado ou executado, nem no host nem no Asterinas.** Ainda faltam resolução de símbolos reais, ABI do kernel e integração do loader.

## Matriz atual

| Componente / alvo | Versão ou formato | Evidência mais recente | Limite |
|---|---|---|---|
| Fonte Asterinas do fork | `0.19.0`, conforme `asterinas/VERSION` | Fonte local e `main` remota conferidas na base `3d6dc25` | Kernel Asterinas completo não recompilado nem inicializado localmente nesta rodada |
| Toolchain do kernel | `nightly-2026-07-21`, conforme `rust-toolchain.toml` | Pin do repositório | Testes no host não substituem build com esse nightly |
| Pré-validador `linux-module-elf` | ELF64 little-endian, ET_REL, x86-64 | **51 testes Rust** e build `no_std`, sem dependências externas ou `unsafe`, aprovados com `rustc 1.97.1`; aplicação offline usa `alloc` | Biblioteca ainda não integrada a syscalls, boot ou loader executável |
| Símbolos / relocations | `SHT_SYMTAB` e subconjunto `SHT_RELA` x86-64 | Fórmulas aplicadas a cópias de dados; overflow, símbolo ausente e sobreposição rejeitados | Valores de símbolos explícitos/sintéticos, sem resolução de exports reais ou execução |
| Objeto C de teste | GCC 13.3.0, ELF ET_REL com 14 seções | Compilação e inspeção aprovadas novamente | Não é um módulo kbuild |
| **Módulo Linux real** | **`cybermanju_fixture.ko` versão `0.1.0`; Linux `6.8.0-147-generic`, x86-64** | **kbuild/MODPOST/link, inspeção das 44 seções e 2.909 escritas RELA em 14 cópias de dados aprovados** | **Endereços sintéticos. Nunca carregado; compatibilidade de execução não comprovada** |
| Outros módulos / versões Linux | Nenhum qualificado | Nenhuma evidência de carregamento, inclusive do módulo mínimo | Não extrapolar essa inspeção para drivers de GPU, outras configurações ou versões Linux |
| ARM64 / RISC-V / LoongArch | Nenhum módulo qualificado | Pré-validador limitado a x86-64; exemplo sintético ARM64 rejeitado | Nenhum build ou execução de `.ko` nessas arquiteturas |
| Ponte NVIDIA NVport | Fonte NVIDIA `615.78.08`, pin `1a82fbfda8f321b056eb3568850f9316c5e465c5` documentado localmente | Sem nova execução NVport nesta rodada | Ponte C/Rust por fontes, não carregamento de `nvidia.ko`; RM completo, GSP e GPU real não validados aqui |
| CI de regressão na base `b241e98` | [Run 39613734176](https://github.com/cybermanju/cybermanju-shell/actions/runs/39613734176) | Jobs `regression`, `linux-module-elf` e `linux-module-kbuild` aprovados | Evidência da base; não substitui o CI da nova aplicação RELA |
| ISO live USB Linux | [Run 39608968083](https://github.com/cybermanju/cybermanju-shell/actions/runs/39608968083), base `3d6dc25` | Gate live USB passou; run avançou à instalação por UEFI CD | Isso valida o instalador Linux, não o boot do Asterinas instalado |
| Instalação / boot instalado | Mesmo run | Instalação em andamento na consulta desta rodada | Instalação concluída e boot instalado ainda não comprovados nessa consulta |

## Primeiro módulo real: exatamente o que foi testado

O código mínimo em [`tests/kbuild`](../asterinas/kernel/libs/linux-module-elf/tests/kbuild/cybermanju_fixture.c) define init que retorna zero, exit vazio e metadados de módulo. Foi compilado pelo **Linux kbuild real**, não pela simples troca de extensão de um objeto C. O executor [`test_linux_module_kbuild.sh`](../tools/ci/test_linux_module_kbuild.sh) não contém comandos de carregamento: só build, `readelf`, `modinfo`, `nm` e inspeção Rust.

A configuração testada foi:

- Headers Ubuntu `linux-headers-6.8.0-147-generic` e comuns `6.8.0-147`, ambos pacote **`6.8.0-147.147`**.
- Compilador **`x86_64-linux-gnu-gcc-13`, Ubuntu `13.3.0-6ubuntu2~24.04.1`**, e inspetor Rust **`1.97.1`**.
- `vermagic`: **`6.8.0-147-generic SMP preempt mod_unload modversions`**.
- Relocations encontradas: **`R_X86_64_32`, `R_X86_64_64`, `R_X86_64_PC32`, `R_X86_64_PLT32`**; todas passaram pelos checks estruturais e foram aplicadas em cópias de dados com endereços sintéticos. O probe confere cada campo escrito e preservação do ELF original; dois testes adicionais no próprio `.ko` rejeitam símbolos externos sem valor fornecido.
- Símbolos ainda indefinidos: **`__fentry__` e `__x86_return_thunk`**. A inspeção não fornece essas funções nem valida os CRCs/contratos de modversions.
- **BTF não gerado**, pois não havia `vmlinux` correspondente. Assinatura/confiança e execução continuam sem validação.

Hashes SHA-256 da amostra local preservada:

| Artefato | SHA-256 |
|---|---|
| `cybermanju_fixture.ko` | `def0f3d8c1490187e2d6f30239a68a911970c7240f475aa351446df04b5d46c6` |
| Fonte C do fixture | `ae0db544d38a724cdac33d74b0d54716628dd23d87bc613dae2f072dd72e3da7` |
| `.config` dos headers | `cf8454bc2e67effab87a5bff3c39042f2d8d106b6a777c4302e52f50d23b7400` |
| `Module.symvers` dos headers | `8e7eb5798789b9610c324b3d9b13373d88c30ae17bb9e41e622efd05e0277a87` |

O `.ko`, logs e metadados completos estão em `artifacts/linux-module-kbuild/`, ignorados pelo Git. `status.txt` registra `exit_code=0` e `loaded=false`; um rerun que falhe grava seu erro para não herdar sucesso de um `provenance.txt` antigo. O build não promete binário idêntico entre diretórios temporários ou toolchains: cada amostra deve ser identificada pelo próprio hash.

## Limites do pré-validador

Reconhece apenas ELF64 little-endian ET_REL x86-64, sem compressão ou numeração estendida. Valida cabeçalho, seções, nomes, symtab/strtab, índices e limites. A RELA aceita larguras dos tipos `NONE` (0 bytes), `64`/`PC64` (8 bytes), `PC32`/`PLT32`/`32`/`32S` (4 bytes); outros tipos e `SHT_REL`/`SHT_RELR` são rejeitados. `NONE` tem política conservadora de offset até o fim da seção. `.bss` usa tamanho em memória.

`inspect` continua somente estrutural. A API separada `relocate_section` calcula `S + A` ou `S + A - P` em `i128`, exige representação exata em `u64`, `i64`, `u32` ou `i32` e retorna um novo `Vec<u8>`. Não cria mapeamentos executáveis nem transforma endereços numéricos em ponteiros. O alvo deve ser PROGBITS ou NOBITS; NOBITS começa zerado. O chamador limita seu tamanho; há teto de 65.536 entradas por seção RELA e falhas de alocação retornam erro. Escritas sobrepostas dentro dessa seção RELA são rejeitadas antes de copiar o alvo; falhas não expõem resultado parcial nem alteram o ELF.

Cada símbolo referenciado, inclusive weak/undefined/common, exige valor explícito; `NONE` não exige resolução. Para `PLT32`, o valor é o destino direto fornecido, sem construir PLT. Não há resolução automática nem política real de exports/COMMON. A operação cobre uma seção RELA por chamada: combinações de várias tabelas sobre o mesmo alvo e layout completo de módulo ainda não estão qualificadas. Permissões executáveis, semântica completa de símbolos/seções, ABI, assinaturas e ciclo de vida continuam pendentes. Testes finitos de truncamento/mutação não equivalem a fuzzing exaustivo ou auditoria formal. Um resultado positivo nunca autoriza chamar código.

## CI e bloqueios de boot

O run anterior [39605334894](https://github.com/cybermanju/cybermanju-shell/actions/runs/39605334894), base `9d13c04`, falhou aguardando readiness por 1.200 segundos: o serial registra timeout de `/dev/ttyS0` e falha da dependência do serial-getty. Não chegou a live-checks ou shutdown; não foi repetição do falso negativo de `grep` do run mais antigo. Os logs não isolam por que o dispositivo não apareceu. O run seguinte avançou além do live USB, mas isso ainda não prova estabilidade do console nem instalação/boot completo.

A concorrência da ISO permanece `cancel-in-progress: false`: o run ativo não deve ser interrompido por pushes periódicos; uma execução pendente ainda pode ser substituída pela mais recente. Resultados são vinculados ao commit testado, sem herdar aprovação entre revisões.

## Testes reproduzíveis

Na raiz, com os headers fixados, GCC 13 e kmod instalados:

```bash
bash tools/ci/test_linux_module_elf.sh
bash tools/ci/test_linux_module_kbuild.sh artifacts/linux-module-kbuild
python3 -m unittest discover -s tools/ci -p 'test_*.py' -v
python3 -m unittest discover -s gpui-shell/scripts -p 'test_frame_telemetry.py' -v
```

Resultados locais: **51 testes Rust**, inspeção e aplicação offline ao objeto C (**4 escritas em 2 seções**) e ao **`.ko` real (2.909 escritas em 14 seções)**, **76 regressões CI/ISO** e **19 regressões de telemetria** aprovados. Onze testes Rust novos cobrem fórmulas, extremos signed/unsigned, símbolo ausente, weak/ABS/COMMON com valores explícitos, `NONE`, NOBITS, teto de entradas, limites, sobreposição e mutações/truncamentos na aplicação. O probe de fixtures rejeita COMMON com diagnóstico explícito: não fornece alocador de símbolos COMMON. O script usa `include/generated/utsrelease.h`, pois o alvo `make kernelrelease` dos headers Ubuntu pode indicar a versão-fonte e não a ABI empacotada. `bash -n tools/ci/*.sh` e `git diff --check` passaram.

O job kbuild existente também executa o probe ampliado. O CI remoto desta alteração deve ser conferido no commit publicado, sem herdar aprovação da base. A ISO da base `b241e98` estava pendente na consulta ([run 39613734180](https://github.com/cybermanju/cybermanju-shell/actions/runs/39613734180)); não foi cancelada. Nenhum novo agendamento foi criado.

## Próxima pequena etapa

Após publicar esta etapa e conferir seu CI, validar um layout conjunto de seções e a resolução explícita de símbolos definidos/ABS, preservando falha para exports ausentes e sem executar código. Nenhuma função do módulo deve ser chamada nessa etapa.

Antes da execução serão necessários confiança/assinatura, privilégio, W^X, ABI/modversions e ciclo de vida/rollback. Módulos Linux executam operações arbitrárias em ring 0: a fronteira de confiança precisa ficar explícita em OSTD, sem introduzir `unsafe` no kernel seguro. Aprovar um módulo mínimo não aprova todos os drivers ou versões do Linux.
