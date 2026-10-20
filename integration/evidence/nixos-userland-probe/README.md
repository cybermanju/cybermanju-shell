# NixOS `hello`: interrompido com segurança antes do build

**Resultado: FAIL / BLOCKED_DISK_CAPACITY. Não é uma falha observada do Asterinas no convidado.** O teste não foi executado, nenhum programa NixOS foi alcançado e nenhum encerramento de convidado foi verificado. O OS-A1 permanece **aberto**.

A árvore examinada foi `/workspace/asterinas-gpui/asterinas`, no commit `00a4842d976db6723b8224be2e57ac8105f13ee2`, com alterações locais preexistentes. Não foi usada outra árvore nem o repositório da aplicação CyberManju.

## O bloqueio

Na coleta final havia **5.668.941.824 bytes disponíveis** no volume. O CI mínimo oficial (`.github/workflows/test_nixos_minimal.yml:46–47`) usa `NIXOS_TEST_SUITE=hello` e `NIXOS_DISK_SIZE_IN_MB=6144`.

O script `tools/nixos/build_nixos.sh:39–41` cria a imagem com `fallocate`, reservando **6.442.450.944 bytes**. Portanto, faltavam **773.509.120 bytes apenas para a imagem**, sem contar crescimento do kernel, initramfs, dependências Nix e instalação. O tamanho padrão do alvo é ainda maior: 16384 MiB. Não se tentou reduzir a imagem nem substituir a alocação por arquivo esparso para contornar essa limitação.

`target/nixos/asterinas.img` não existe. Os artefatos OSDK encontrados são kernels/ISOs de outras execuções; sua existência não comprova um rootfs NixOS válido. Foram inventariados e receberam hashes, mas não foram reutilizados nem alterados. A imagem esperada é obrigatória no runner (`tools/nixos/run.sh`, modo `nixos`).

O alvo raiz `make nixos` executa primeiro `make kernel`, depois o wrapper em `test/nixos`; por segurança, nenhum deles foi iniciado. O wrapper normalmente gera uma configuração de teste temporária, mas nenhuma foi gerada por esta sondagem. Não se executou nem mesmo um dry-run recursivo de Make que pudesse acionar receitas.

Também foram observados: Nix 2.18.1, QEMU 8.2.2, Make 4.3, ausência de `/dev/kvm` e ausência do executável Docker. `AGENTS.md` prescreve o contêiner do projeto; esse ambiente deve ser providenciado antes de uma futura execução. A capacidade insuficiente é o bloqueio decisivo já demonstrado; outros pré-requisitos do build não foram exercitados.

## O que o teste oficial verificaria

O runner documentado em `test/nixos/README.md:100–142` usa `make nixos` e `make run_nixos`, com a suíte `hello`. `test/nixos/tests/hello` não tem `extra_config.nix`, portanto usa a configuração base, que inclui `hello-asterinas`.

O framework (`test/nixos/common/framework/src/lib.rs:214–225`) espera o prompt root `root@asterinas:`. O caso `hello` (`test/nixos/tests/hello/src/main.rs:9–13`) executa `hello-asterinas` e espera `Hello Asterinas!`, seguido do retorno do prompt. A função de comparação verifica o texto, não o status de saída do programa (`session.rs:137–165`).

Ao final, o framework envia `poweroff` e espera o processo terminar (`session.rs:333–340`). O script QEMU normaliza os códigos 0 e 33 como sucesso (`tools/nixos/run.sh`). Para a exigência de encerramento limpo, uma execução futura deverá preservar também o log de shutdown e o resultado do processo: o simples resumo de casos aprovados não comprova sozinho toda essa exigência. Não houve interação manual nem tentativa de adivinhar prompts.

## Retomada

Os comandos oficiais e a adaptação proposta para execução headless sem KVM estão em `commands-not-executed.txt`, claramente marcados como **não executados**. Antes de retomá-los, aumentar a capacidade disponível, deixando espaço para a imagem de 6 GiB **mais** builds e o store Nix, e disponibilizar o ambiente oficial. Não foi calculado um limite total seguro de espaço. Nenhum cache foi apagado e não houve pedido de limpeza automática.

O escopo pretendido é somente **CPU/QEMU x86_64**, como smoke test do userspace Linux/rootfs NixOS sobre Asterinas. Nem esta coleta nem uma futura aprovação de `hello` comprovam os requisitos de libc, GTK/WebKit/Tauri, Wayland, GPU ou compatibilidade da aplicação CyberManju. Não se declara integração completa.

## Evidências

- `summary.json`: resultado explícito, escopo, espaço, ferramentas e critérios não verificados.
- `stdout.log` e `commands-executed.txt`: comandos somente leitura e suas saídas/códigos de retorno.
- `commands-not-executed.txt`: comandos previstos, sem alegação de execução.
- `inspection-*.log`: saídas da inspeção inicial dos alvos, scripts, host e artefatos.
- `input-sha256.json`: hashes dos inputs NixOS, arquivos locais alterados/não rastreados e metadados relevantes; não é um snapshot completo recuperável da árvore.
- `existing-artifacts.json`: tamanhos e hashes dos artefatos principais Asterinas encontrados; nenhum foi validado para este boot.
- `git-status-before.txt`, `git-status-after.txt` e `preservation.json`: estado Git idêntico e todos os inputs registrados inalterados durante a coleta.
- `collect_preflight.py`: coletor reproduzível somente leitura; não faz build nem inicia QEMU. Reexecutá-lo atualiza os arquivos de coleta deste diretório.
- `serial-status.txt`: explica por que não há captura serial; não contém saída inventada de convidado.
- `SHA256SUMS`: integridade dos arquivos desta evidência.

Somente este diretório de evidência foi criado. Não houve edição de kernel, fontes ou configuração, instalação Cargo/npm, build direto, staging, commit, push, PR ou execução remota de Actions. Nenhum token, valor secreto ou dump do ambiente foi incluído.
