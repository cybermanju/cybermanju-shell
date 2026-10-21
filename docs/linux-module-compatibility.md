# Módulos Linux: o que funciona e o que falta provar

**Nenhum módulo Linux `.ko` foi demonstrado carregando neste kernel.** A nova biblioteca Rust valida parte da estrutura de um arquivo ELF; não é um loader nem uma promessa de compatibilidade com drivers Linux.

## Matriz atual

| Componente / alvo | Versão ou formato | Evidência desta execução | Limite |
|---|---|---|---|
| Fonte Asterinas do fork | `0.19.0`, conforme `asterinas/VERSION` | Inspeção do código local, base `8594bcf` | Kernel completo não recompilado nem inicializado nesta execução |
| Toolchain do kernel | `nightly-2026-07-21`, conforme `rust-toolchain.toml` | Pin lido no repositório | Toolchain não instalada neste ambiente; testes abaixo não substituem build com esse nightly |
| Pré-validador `linux-module-elf` | ELF64 little-endian, ET_REL, x86-64 | 14 testes Rust aprovados em host Linux x86-64, `rustc 1.97.1`; build `no_std` sem dependências e sem `unsafe` aprovado | Não integrado a syscalls ou ao caminho de boot; símbolos e relocations ainda não interpretados |
| Objeto C real de teste | GCC 13.3.0, ELF ET_REL x86-64 com 14 seções | Compilação e inspeção aprovadas; `readelf` confirmou `.bss`, `.modinfo`, símbolos e relocations | Objeto C comum, **não** `.ko` produzido por Linux kbuild; nenhuma relocação aplicada e nenhum código executado |
| ARM64 / RISC-V / LoongArch | Nenhuma versão de módulo qualificada | Arquitetura diferente rejeitada pelo pré-validador; exemplo sintético ARM64 | Nenhuma execução de módulo nessas arquiteturas |
| Módulos Linux reais | Nenhum nome/versão aprovado | Nenhum artefato `.ko` encontrado na árvore Asterinas local | Kernel/configuração Linux alvo, `Module.symvers`, `vermagic` e assinaturas ainda sem qualificação |
| Ponte NVIDIA NVport | Fonte NVIDIA `615.78.08`, pin `1a82fbfda8f321b056eb3568850f9316c5e465c5` documentado na biblioteca | Somente inspeção da documentação nesta execução | Integração experimental por fontes C/Rust; não carrega `nvidia.ko`. RM completo, GSP e GPU real não validados aqui |
| ISO / instalação / boot instalado | CI remoto | Consulta e publicação bloqueadas pela política de autenticação da ferramenta nesta execução | Não foi possível confirmar o estado atual do CI ou da `main` remota; resultados anteriores não são reclassificados como sucesso |

## Melhoria implementada

A biblioteca em [`asterinas/kernel/libs/linux-module-elf`](../asterinas/kernel/libs/linux-module-elf/src/lib.rs) é `no_std`, sem alocação ou dependências externas, e proíbe `unsafe`. Verifica identidade ELF, classe/endianness/versão, tipo relocável e arquitetura, tamanhos do cabeçalho, ausência de program headers, limites da tabela de seções, seção nula, alinhamentos e intervalos de payload com soma verificada. Valida também a tabela de nomes. Rejeita numeração estendida e seções comprimidas; não tenta ler `.bss`/`SHT_NOBITS` como bytes do arquivo.

As entradas malformadas incluem truncamentos em todos os prefixos da amostra, overflow, intervalos fora do arquivo, metadados inconsistentes, nomes inválidos e mutações de bytes. Esses testes finitos **não** equivalem a fuzzing exaustivo ou auditoria formal.

O pré-validador não verifica ainda links entre símbolos e relocations, semântica das flags/seções, sobreposições de regiões, ABI Linux ou autenticidade. Um retorno positivo **nunca** deve autorizar mapeamento executável ou chamada de entrada de módulo.

## Testes reproduzíveis

Na raiz do repositório:

```bash
bash tools/ci/test_linux_module_elf.sh
python3 -m unittest discover -s tools/ci -p 'test_*.py' -v
python3 -m unittest discover -s gpui-shell/scripts -p 'test_frame_telemetry.py' -v
```

Resultados locais: **14 testes Rust**, inspeção do objeto C compilado, **66 regressões de CI/ISO** e **19 regressões de telemetria** aprovados. `bash -n tools/ci/*.sh` e `git diff --check` aprovados. Não há nova evidência de boot, instalação ou GPU. Os logs locais estão em `artifacts/linux-module-elf/` (ignorados pelo Git).

O workflow `checks.yml` ganhou um job próprio para esses testes estruturais, com Rust `1.97.1`; esse job remoto ainda não foi executado com a alteração desta rodada.

## Próxima pequena etapa

Validar relações e limites de `.symtab`/`.strtab` e das tabelas de relocations x86-64, com testes negativos para índices e tamanhos inválidos, ainda sem execução. Depois fixar um kernel Linux/configuração e compilar um módulo mínimo real via kbuild, mantendo separadas as etapas de inspeção, resolução, relocation e execução.

Antes de habilitar execução serão necessários uma política de confiança/assinatura, controle de privilégio, W^X, ciclo de vida e rollback de falhas. Códigos de módulos Linux podem executar operações arbitrárias em ring 0: a fronteira de confiança precisa ficar explícita em OSTD, sem introduzir `unsafe` no kernel seguro. Aprovar um módulo mínimo não aprova todos os drivers ou versões do Linux.
