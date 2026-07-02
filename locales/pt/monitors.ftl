# Interface de monitores: a lista de monitores (check-ins de cron) por projeto e
# a página de detalhe do monitor. Reutiliza nav-monitors. As strings com contagem
# usam plurais tv_count ([one]/[other]).

# --- Sufixo do título da página ---
monitors-title-suffix = — Stackpit

# --- Lista de monitores ---
monitors-list-empty = Nenhum monitor encontrado. Os eventos de check-in com um <code class="text-mono">monitor_slug</code> aparecerão aqui.
monitors-col-slug = Slug
monitors-col-last-status = Último estado
monitors-col-last-checkin = Último check-in
monitors-col-count = Contagem

# --- Detalhe do monitor ---
monitors-detail-title-prefix = Monitor
monitors-detail-subtitle = Check-ins do monitor.
monitors-detail-empty = Nenhum check-in encontrado para este monitor.
monitors-detail-select-checkin = Selecionar check-in
monitors-detail-confirm-delete-selected = Eliminar os check-ins selecionados?
monitors-detail-delete = Eliminar
monitors-detail-col-title = Título
monitors-detail-col-level = Nível
monitors-detail-col-environment = Ambiente
monitors-detail-col-time = Hora
monitors-detail-untitled = (sem título)
monitors-detail-confirm-delete-all = { $count ->
    [one] Eliminar todos os { $count } check-ins?
   *[other] Eliminar todos os { $count } check-ins?
}
monitors-detail-delete-all = { $count ->
    [one] Eliminar todos os { $count }
   *[other] Eliminar todos os { $count }
}

# --- Paginação ---
monitors-pagination-label = Paginação
monitors-pagination-prev = « Anterior
monitors-pagination-next = Seguinte »
monitors-detail-count = { $count ->
    [one] { $count } check-in
   *[other] { $count } check-ins
}
