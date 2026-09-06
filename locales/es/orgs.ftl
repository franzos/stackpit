# Superficie de organizaciones: la lista de orgs (templates/orgs.html), la página
# de miembros/invitaciones (templates/org_members.html) y la página de aceptación
# de invitación (templates/invite_accept.html, claves invite-*). Reutiliza
# nav-organizations y common-action-save. Los espacios separadores viven en el
# template. La frase de peligro de borrado y la etiqueta de confirmación se dividen
# en sus interpolaciones {{ var }}. Los valores enum (member/owner, estado)
# permanecen literales en el template.
orgs-page-title = Organizaciones - Stackpit
orgs-subtitle = Las organizaciones a las que perteneces. Cambia entre ellas o crea una nueva.
orgs-empty = Aún no eres miembro de ninguna organización.
orgs-col-organization = Organización
orgs-col-kind = Tipo
orgs-members-btn = Miembros
orgs-active = Activa
orgs-switch = Cambiar
orgs-create-heading = Crear organización
orgs-create-desc = Te conviertes en el propietario. El slug se deriva del nombre si se deja en blanco.
orgs-name = Nombre
orgs-slug = Slug
orgs-optional = (opcional)
orgs-create-submit = Crear organización

# --- Página de miembros ---
orgs-members-title-suffix = Miembros - Stackpit
orgs-members-word = Miembros
orgs-organization-word = organización
orgs-slug-heading = Slug
orgs-slug-desc = Identifica esta organización en las URLs. Debe ser único.
orgs-email = Correo electrónico
orgs-role = Rol
orgs-role-member = miembro
orgs-role-owner = propietario
orgs-member-fallback = usuario n.º { $id }
orgs-joined = Se unió
orgs-promote = Ascender
orgs-demote = Degradar
orgs-remove = Quitar
orgs-invites-heading = Invitaciones
orgs-created = Creada
orgs-expires = Caduca
orgs-status = Estado
orgs-revoke = Revocar
orgs-create-invite-heading = Crear invitación
orgs-create-invite-desc = Genera un enlace de invitación de un solo uso.
orgs-expiry-label = Caducidad (segundos)
orgs-expiry-hint = (opcional, 7 días por defecto)
orgs-create-invite-submit = Crear invitación
orgs-forseti-note = La membresía de esta organización se gestiona externamente.
orgs-personal-note = Esta es una organización personal. La membresía no es configurable.
orgs-danger-heading = Zona de peligro
orgs-delete-danger-pre = Al eliminar se quitan
orgs-delete-danger-projects = proyecto(s),
orgs-delete-danger-members = miembro(s),
orgs-delete-danger-rest = y todos los eventos, problemas, claves, alertas e integraciones. Esto no se puede deshacer.
orgs-confirm-type-pre = Escribe
orgs-confirm-type-post = para confirmar
orgs-delete-confirm = Elimina esta organización y TODOS sus datos. Esto no se puede deshacer.
orgs-delete-submit = Eliminar organización

# --- Aceptar invitación (página independiente) ---
invite-page-title = Invitación a organización - Stackpit
invite-heading = Invitación a organización
invite-back-projects = Volver a los proyectos
invite-intro-pre = Se te ha invitado a unirte a
invite-intro-as = como
invite-intro-post = .
invite-accept-btn = Aceptar invitación
invite-decline = Rechazar
invite-error-accepted = Esta invitación ya ha sido aceptada.
invite-error-expired = Esta invitación ha caducado.
invite-error-email-mismatch = Esta invitación es para otra dirección de correo. Pide una invitación sin restricción de correo o inicia sesión con la cuenta correspondiente.

# Mensajes de validación/error renderizados en la página html_error, localizados
# en los puntos de llamada que llevan un locale de solicitud. Los errores 5xx
# internos permanecen en inglés.
orgs-err-name-required = El nombre de la organización es obligatorio.
orgs-err-slug-taken = Ese slug ya está en uso.
orgs-err-invite-not-found = Invitación no encontrada o no válida.
orgs-err-org-not-found = Organización no encontrada.
orgs-err-last-owner-remove = No se puede quitar al último propietario.
orgs-err-last-owner-demote = No se puede degradar al último propietario.
orgs-err-confirm-slug = Escribe el slug de la organización para confirmar la eliminación.
orgs-err-not-deletable = Esta organización no se puede eliminar.
orgs-err-license-cap-reached = Se alcanzó el límite de organizaciones de tu licencia. Elimina una organización o amplía la licencia para crear otra.
orgs-err-limit-reached = { $count ->
    [one] Has alcanzado el límite de { $count } organización.
   *[other] Has alcanzado el límite de { $count } organizaciones.
}
