# Página de início de sessão (templates/login.html) e as strings de banner de
# OAuth/logout produzidas em src/html/login.rs. login-token-help contém markup
# <code> inline e é renderizado com |safe.
login-page-title = Iniciar sessão — Stackpit
login-welcome = Bem-vindo de volta
login-subtitle = Inicie sessão para gerir o seu rastreio de erros
login-sso = Iniciar sessão com SSO
login-or = ou
login-token-label = Token de administrador
login-token-placeholder = Introduza o seu token principal…
login-submit = Iniciar sessão
login-token-help = O token de administrador provém de <code class="text-mono">admin_token</code> em <code class="text-mono">stackpit.toml</code>. Edite o ficheiro e reinicie <code class="text-mono">stackpit serve</code> para aplicar as alterações.
login-docs = Documentação
login-selfhosting = Guia de auto-alojamento

# Banner de erro (mapeado a partir dos códigos ?error= de redirecionamento OAuth) e banner de logout.
login-error-state-mismatch = A sua sessão de início foi adulterada ou expirou. Tente novamente.
login-error-session-expired = A sua sessão expirou. Inicie sessão novamente.
login-error-missing-response = O seu fornecedor de identidade devolveu uma resposta incompleta. Tente novamente.
login-error-token-exchange = Não foi possível concluir o início de sessão com o seu fornecedor de identidade. Tente novamente dentro de momentos.
login-error-provisioning = Não foi possível criar a sua conta. Contacte o seu administrador.
login-error-email-conflict = Já existe uma conta com este e-mail. Contacte o seu administrador.
login-error-session-unavailable = O início de sessão está temporariamente indisponível. Tente novamente dentro de momentos.
login-error-encryption = O início de sessão está mal configurado nesta instância. Contacte o seu administrador.
login-error-generic = O início de sessão falhou. Tente novamente.
login-error-invalid-token = Token inválido
login-logout-local = Sessão terminada no Stackpit. A sua sessão no fornecedor de identidade não foi terminada -- termine-a lá separadamente, se necessário.
