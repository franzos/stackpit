# Interface de organizações: a lista de organizações (templates/orgs.html), a
# página de membros/convites (templates/org_members.html) e a página autónoma de
# aceitação de convite (templates/invite_accept.html, chaves invite-*). Reutiliza
# nav-organizations e common-action-save. Os espaços separadores estão no template.
# A frase de perigo de eliminação e o label "Escreva <slug> para confirmar" são
# divididos nas interpolações {{ var }}. Os valores enum (member/owner, estado)
# permanecem literais no template.
orgs-page-title = Organizações - Stackpit
orgs-subtitle = As organizações a que pertence. Alterne entre elas ou crie uma nova.
orgs-empty = Ainda não é membro de nenhuma organização.
orgs-col-organization = Organização
orgs-col-kind = Tipo
orgs-members-btn = Membros
orgs-active = Ativa
orgs-switch = Mudar
orgs-create-heading = Criar organização
orgs-create-desc = Torna-se o proprietário. O slug é derivado do nome quando deixado em branco.
orgs-name = Nome
orgs-slug = Slug
orgs-optional = (opcional)
orgs-create-submit = Criar organização

# --- Página de membros ---
orgs-members-title-suffix = Membros - Stackpit
orgs-members-word = Membros
orgs-organization-word = organização
orgs-slug-heading = Slug
orgs-slug-desc = Identifica esta organização nos URLs. Deve ser único.
orgs-email = E-mail
orgs-role = Função
orgs-role-member = membro
orgs-role-owner = proprietário
orgs-member-fallback = utilizador #{ $id }
orgs-joined = Aderiu
orgs-promote = Promover
orgs-demote = Despromover
orgs-remove = Remover
orgs-invites-heading = Convites
orgs-created = Criado
orgs-expires = Expira
orgs-status = Estado
orgs-revoke = Revogar
orgs-create-invite-heading = Criar convite
orgs-create-invite-desc = Gera uma ligação de convite de utilização única.
orgs-expiry-label = Validade (segundos)
orgs-expiry-hint = (opcional, predefinição 7 dias)
orgs-create-invite-submit = Criar convite
orgs-forseti-note = A adesão a esta organização é gerida externamente.
orgs-personal-note = Esta é uma organização pessoal. A adesão não é configurável.
orgs-danger-heading = Zona de perigo
orgs-delete-danger-pre = A eliminação remove
orgs-delete-danger-projects = projeto(s),
orgs-delete-danger-members = membro(s),
orgs-delete-danger-rest = e todos os eventos, problemas, chaves, alertas e integrações. Isto não pode ser anulado.
orgs-confirm-type-pre = Escreva
orgs-confirm-type-post = para confirmar
orgs-delete-confirm = Eliminar esta organização e TODOS os seus dados. Isto não pode ser anulado.
orgs-delete-submit = Eliminar organização

# --- Aceitar convite (página autónoma) ---
invite-page-title = Convite de organização - Stackpit
invite-heading = Convite de organização
invite-back-projects = Voltar aos projetos
invite-intro-pre = Foi convidado para aderir a
invite-intro-as = como
invite-intro-post = .
invite-accept-btn = Aceitar convite
invite-decline = Recusar
invite-error-accepted = Este convite já foi aceite.
invite-error-expired = Este convite expirou.

# Mensagens de validação/erro renderizadas na página html_error, localizadas nos
# pontos de chamada que carregam um locale de pedido. As falhas 5xx internas
# permanecem em inglês.
orgs-err-name-required = O nome da organização é obrigatório.
orgs-err-slug-taken = Esse slug já está em utilização.
orgs-err-invite-not-found = Convite não encontrado ou inválido.
orgs-err-org-not-found = Organização não encontrada.
orgs-err-last-owner-remove = O último proprietário não pode ser removido.
orgs-err-last-owner-demote = O último proprietário não pode ser despromovido.
orgs-err-confirm-slug = Escreva o slug da organização para confirmar a eliminação.
orgs-err-not-deletable = Esta organização não pode ser eliminada.
orgs-err-limit-reached = { $count ->
    [one] Atingiu o limite de { $count } organização.
   *[other] Atingiu o limite de { $count } organizações.
}
