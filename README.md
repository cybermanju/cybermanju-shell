# cybermanju-shell

Monorepo experimental que reúne o código-fonte do **Asterinas** e do **GPUI Shell**, incluindo as alterações locais de gráficos, NVIDIA e integração NixOS. Os dois componentes são árvores completas de código, não submódulos.

**Estado atual: consolidação de fontes e trabalho experimental. O GPUI Shell ainda não executa sobre Asterinas. Este repositório não entrega um sistema operacional gráfico integrado, nem um driver NVIDIA funcional completo.**

## O que está incluído

| Diretório | Conteúdo |
| --- | --- |
| `asterinas/` | Kernel, OSTD, documentação, ferramentas e alterações locais de gráficos/PCI/DMA/NVIDIA. |
| `gpui-shell/` | Shell Wayland em Rust/GPUI, documentação, recursos originais e alterações locais de NixOS/áudio. |
| `local-prs/` | Histórico de patches e `PR_NOTES.md` preservados do workspace anterior. |
| `integration/snapshot/` | Origens, commits-base, hashes de arquivos, status original, diffs das alterações em arquivos versionados e resultado dos testes desta preparação. |
| `tools/verify_snapshot.py` | Verificação dos arquivos preservados contra o manifesto. |

Os componentes mantêm seus próprios workspaces Cargo, toolchains e configurações Nix. Execute os comandos dentro do componente correspondente; não existe um workspace Cargo unificado na raiz. Workflows originais dentro de `asterinas/.github/` e `gpui-shell/.github/` foram preservados como fonte, mas **não são executados automaticamente pelo GitHub Actions a partir desses subdiretórios**.

## Origens e alterações preservadas

- [Asterinas original](https://github.com/asterinas/asterinas), fork [cybermanju/asterinas](https://github.com/cybermanju/asterinas), base `00a4842d976db6723b8224be2e57ac8105f13ee2`.
- [GPUI Shell original](https://github.com/andre-brandao/gpui-shell), fork [cybermanju/gpui-shell](https://github.com/cybermanju/gpui-shell), base `fb4173c9c23cd8dc16732f800ba4921eb1233a50`.

O snapshot usa o conteúdo dos worktrees, incluindo arquivos novos e alterações não commitadas; não apenas os commits-base. O histórico Git completo dos projetos não foi importado. As notas antigas descrevem o estado na ocasião em que foram escritas e não substituem o manifesto deste snapshot. Patches históricos podem se sobrepor e já estão refletidos nas fontes: não os aplique indiscriminadamente.

No Asterinas, o trabalho inclui correções de dumb buffers/simpledrm, diagnóstico gráfico, ferramentas Vulkan compute e infraestrutura NVIDIA experimental opt-in: NVport, PCI, posse de recursos, DMA e políticas VT-d/PCIe restritas. No GPUI Shell, inclui módulo NixOS opt-in e tratamento de falha de áudio sem abortar o shell.

## Verificação rápida

Na raiz, confira a preservação dos arquivos:

```sh
python3 tools/verify_snapshot.py
```

Execute os testes Python das ferramentas gráficas:

```sh
cd asterinas
python3 -m unittest discover -s tools/graphics -v
```

Na preparação deste snapshot, **38 testes passaram**. O log está em `integration/snapshot/graphics-unit-tests.log`; mensagens de falha de pré-requisitos no fim do log fazem parte de fixtures dos testes. Isso não comprova boot, aceleração ou integração gráfica.

Para diagnosticar o ambiente (pode reportar pré-requisitos ausentes):

```sh
cd asterinas
python3 tools/graphics/check_graphics.py
```

Para os testes de serviços do shell, com o toolchain e dependências do projeto instalados:

```sh
cd gpui-shell
cargo test -p services
```

Para construir o shell a partir deste checkout, em uma máquina com Nix e flakes habilitados:

```sh
cd gpui-shell
nix build
nix run .
```

A execução do shell exige uma sessão Wayland compatível. Consulte `gpui-shell/README.md` e `gpui-shell/docs/src/content/docs/developer/setup.md`. A preparação do monorepo não executou novamente os builds Rust/Nix, testes de kernel ou testes em GPU física.

## Limites importantes

Ainda faltam as interfaces e serviços necessários para executar GPUI/Wayland com renderização Vulkan acelerada no Asterinas. O caminho NVIDIA não implementa o ciclo RM/GSP completo, sua ABI ioctl/mmap e toda a infraestrutura DRM/GEM/sincronização/modosetting necessária. Testes positivos com dispositivos QEMU EDU/NVMe não demonstram suporte a GPUs NVIDIA reais.

As evidências anteriores descrevem testes CPU/QEMU e shell aninhado com Mesa/llvmpipe. Não são certificação de aceleração por GPU física nem de compatibilidade do GPUI com Asterinas. Consulte `asterinas/tools/graphics/README.md`, `CI.md` e `NATIVE_NVIDIA.md` antes de qualquer teste nativo.

## O que ficou de fora

Não foram copiados diretórios `.git/`, `target/`, caches, instalações locais, toolchains, a pasta `evidence/`, imagens de boot ou firmware não verificado. Os recursos visuais originais versionados do GPUI Shell foram mantidos.

O checkout externo NVIDIA em `vendor-drivers/` não faz parte dos dois projetos consolidados e não foi incorporado. A integração NVport requer separadamente fontes NVIDIA confiáveis da versão/commit documentados e, para testes de hardware, userspace e firmware compatíveis. Nenhum download ou validação desse material foi realizado nesta preparação.

## Licenças e avisos

O Asterinas é principalmente MPL-2.0, com exceções descritas em `asterinas/.licenserc.yaml`; veja `asterinas/LICENSE-MPL` e os cabeçalhos de cada arquivo. Todos os avisos originais foram preservados.

Não foi encontrado um arquivo de licença de nível superior no snapshot do GPUI Shell. Sua inclusão integral foi autorizada pelo solicitante; isso **não** atribui automaticamente MPL-2.0, MIT ou outra licença a esse código. Preserve os avisos existentes e esclareça os termos antes de redistribuição pública. Este monorepo não declara uma licença única sobre todos os componentes.

O projeto GPUI Shell é experimental e seu README original contém avisos sobre desenvolvimento inicial, código gerado por IA e ausência de afiliação oficial à NixOS Foundation. Esses avisos foram mantidos.
