# Segurança no Zek Download (Windows)

## Como o motor (yt-dlp) e as ferramentas são obtidos
- yt-dlp: baixado **somente** de `https://github.com/yt-dlp/...` (releases oficiais), conferido por SHA-256
  contra o `SHA2-256SUMS` da mesma release e testado (`--version`) antes de substituir o anterior.
- ffmpeg: build oficial do projeto yt-dlp (`yt-dlp/FFmpeg-Builds`), com SHA-256 do `checksums.sha256`.
- Deno (interpretador JS exigido pelo YouTube): release oficial do `denoland/deno`, com SHA-256.
- Conexões somente HTTPS (redirecionamento para http é recusado); certificados verificados; limites de tamanho.
- Tudo é gravado em pasta do usuário (`%LOCALAPPDATA%\ZekDownload\bin`, ou `data\bin` no modo portátil).
  Nenhuma elevação de administrador em tempo de execução.

**Limite honesto:** o checksum vem da mesma release do arquivo. Protege contra corrupção e troca em trânsito,
não contra o comprometimento da própria conta do fornecedor no GitHub.

## Execução
- O yt-dlp roda **sem shell**, com lista de argumentos; o link vai depois de `--`.
- Só aceita links `http://` e `https://`; rejeita espaços, caracteres de controle e links gigantes.
- `--ignore-config`: configurações do usuário (que poderiam ter `--exec`) são ignoradas.
- Opções validadas contra listas fixas; arquivo de configurações validado ao carregar.
- O ZIP baixado só extrai arquivos com nomes fixos (nunca usa caminhos de dentro do ZIP).

## Windows SmartScreen
O `.exe` **não é assinado digitalmente** (certificado de assinatura é pago). Na primeira execução o Windows pode
mostrar "O Windows protegeu o computador": clique em **Mais informações → Executar assim mesmo**.
Confira o `SHA256SUMS.txt` da release se quiser garantir a integridade.
