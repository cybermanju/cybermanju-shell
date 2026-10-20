# Fontes oficiais consultadas

- NVIDIA releases: https://github.com/NVIDIA/open-gpu-kernel-modules/releases — marca 615.78.08 como Latest, commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`. A fonte revisada local não foi substituída e nenhum firmware foi carregado.
- Rust Box: https://doc.rust-lang.org/std/boxed/index.html — valores zero-sized precisam somente de ponteiro não nulo e alinhado; o endereço de dados não identifica uma alocação exclusiva.
- Rust Arc: https://doc.rust-lang.org/std/sync/struct.Arc.html — `Arc::ptr_eq` compara a identidade da alocação e ignora metadata de trait objects. A implementação usa Arc do alloc já usado pelo OSTD, sem nova dependência.

As páginas foram abertas e lidas, não apenas inferidas dos resultados de busca. Não houve login ou uso de token.
