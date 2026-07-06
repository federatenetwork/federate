# Política de Segurança

> [English version](SECURITY.md)

A Federate Network é infraestrutura crítica de segurança: um sistema de nomes
e conteúdo onde toda camada (zona raiz, registros de TLD, registros de
domínio, manifests, blocos de conteúdo) é verificada criptograficamente.
Bugs que quebram essa cadeia importam muito.

## Reportando uma vulnerabilidade

Por favor, **não** abra uma issue pública para problemas de segurança.

Use o relato privado de vulnerabilidades do GitHub: aba **Security → Report a
vulnerability** neste repositório. Você recebe resposta assim que possível e
crédito nas notas da correção, a menos que prefira o contrário.

Relatos especialmente interessantes:

- bypass de verificação de assinatura ou hash em qualquer camada
- aceitação de rollback/replay da zona raiz
- path traversal nos stores de blocos/manifests
- spoofing de respostas DNS ou cache poisoning no `federate-dns`
- SSRF via registro de nós ou busca de blocos em providers
- envenenamento do diretório (nós falsos, poluição do mapa de providers)

## Notas de escopo

- Chaves privadas nunca saem do nó que as gerou; qualquer API que exponha uma
  é um bug crítico.
- Nós não são confiáveis por design: um relato precisa mostrar um *cliente*
  aceitando dados ruins, não só um nó servindo eles.
- Orientação de hardening de implantação vive em
  [docs/pt-BR/deployment-vps.md](docs/pt-BR/deployment-vps.md).
