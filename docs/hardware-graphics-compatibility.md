# Gráficos e hardware: o que já foi provado

**O framebuffer de boot já passou por escrita e leitura de pixels em uma VM QEMU.** A medição final foi **1280 × 800, 32 bpp, pitch de 5.120 bytes e superfície de 4.096.000 bytes**. Não é uma demonstração de GPUI nativo, aceleração gráfica ou imagem apresentada em um monitor físico.

## Correção que sustenta o teste

O [descritor de boot](../asterinas/ostd/src/boot/mod.rs) agora preserva o pitch fornecido pelo bootloader, em vez de supor que cada linha ocupa exatamente largura × bytes por pixel. As conversões de Linux boot, Multiboot v1 e Multiboot2 transportam esse valor. O span reservado passa a ser `pitch × height`, incluindo padding; dimensões nulas, pitch insuficiente, formatos fora da lista admitida e overflow são recusados antes de publicar os metadados.

O [consumidor gráfico](../asterinas/kernel/core/comps/framebuffer/src/framebuffer.rs) usa esse pitch, valida o arredondamento do mapeamento e trata indisponibilidade do MMIO sem panic. O clear usa blocos de até uma página, sem alocar outra superfície inteira nem limpar a cauda de arredondamento da última página. Isso não significa que todas as APIs públicas de escrita imponham o limite lógico da superfície: o `IoMem` ainda cobre páginas completas.

O [header Multiboot2](../asterinas/ostd/src/arch/x86/boot/multiboot2/header.S) solicita framebuffer **opcional**, com preferência por 32 bits e geometria escolhida pelo loader. Sem a solicitação, o GRUB podia entregar apenas EGA/texto, que o parser corretamente rejeita como framebuffer de pixels. A versão final usa o GRUB padrão, sem wrapper, fonte ou terminal gráfico customizado.

## Evidência executável

| Caminho | Resultado observado | Limite |
|---|---|---|
| Contrato de boot | 1 ktest aprovado, zero falhas: padding sintético, span da região, overflow e conversão real de tags Multiboot2 | Não realiza I/O de GPU; o caso com padding é sintético |
| Framebuffer VGA de QEMU | 1 ktest aprovado, zero falhas: inicialização real, clear amostrado, escrita/leitura/restauração de dois pixels na primeira e última linha | Readback do aperture MMIO, não scanout; o dispositivo observado não tem padding de linha |
| Adaptadores de produção | `cargo check --locked -p ostd -p linux-module-staging -p aster-framebuffer --target x86_64-unknown-none` aprovado | Type-check, não boot do kernel completo |
| Regressão de frames de módulos | 1 ktest / 5 cenários aprovado novamente no header final | Nenhum `.ko` executado |

Ambas as VMs gráficas mínimas usam x86-64/q35, TCG, 1 CPU, 1 GiB e BIOS/GRUB Multiboot2; QEMU `8.2.2`, Rust `1.99.0-nightly`, pin `nightly-2026-07-21`. São imagens de teste OSTD, **não a ISO instaladora nem a instalação UEFI completa**.

Reprodução a partir da raiz:

```sh
bash tools/ci/test_framebuffer_runtime.sh artifacts/framebuffer-contract --kind contract
bash tools/ci/test_framebuffer_runtime.sh artifacts/framebuffer-hardware --kind hardware
```

O [runner](../tools/ci/test_framebuffer_runtime.sh) recusa configurações preexistentes, invalida sucesso antigo, limita a VM a 600 segundos e reconstrói apenas o bundle descartável. Cada diretório guarda serial, JSON verificado, configuração GRUB extraída da imagem, manifests/lockfile do harness, versões e hashes. O [verificador](../tools/ci/verify_framebuffer_runtime.py) exige marcadores completos únicos e exatamente um teste aprovado, rejeitando evidência ausente, contraditória ou vazia. O job `framebuffer-runtime` repete ambos os modos e retém esses registros no CI; sua inclusão não equivale a já ter passado remotamente.

## Limites e próxima integração

A geometria aceita no descritor pode ter endereço desalinhado, mas o consumidor atual **ainda recusa endereço físico não alinhado a página**. Os formatos de cor continuam inferidos por bpp; máscaras e ordem de canais não são preservadas. Linux boot, Multiboot v1, 8/16/24 bpp e GPU física não ganharam runtime de qualificação nesta rodada.

O próximo passo gráfico é preservar o formato completo de cor e tratar mapeamento com offset de página, depois validar apresentação DRM/input e o caminho GPUI↔kernel com evidência de ponta a ponta. Readback de bytes não comprova cores, sincronização de apresentação ou FPS. O [loader de módulos](linux-module-compatibility.md) segue separado: faltam placement virtual real, proteção de páginas levando em conta o alias linear e execução controlada de init.
