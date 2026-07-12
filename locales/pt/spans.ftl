# Interface de spans: a lista de spans/traces por projeto (spans-*) e a página de
# detalhe do waterfall do trace (trace-detail-*). Reutiliza nav-spans. As strings
# com contagem usam plurais tv_count ([one]/[other]).

# --- Sufixo do título da página ---
spans-title-suffix = — Stackpit

# --- Lista de spans/traces ---
spans-list-empty = Nenhum span encontrado para este projeto.
spans-traces-heading = Traces
spans-all-heading = Todos os spans

# --- Tabela de traces ---
spans-col-trace-id = ID do trace
spans-col-root-op = Op raiz
spans-col-root-description = Descrição raiz
spans-col-duration = Duração
spans-col-first-seen = Visto pela primeira vez
spans-col-last-seen = Visto pela última vez

# --- Tabela de spans agregados (agrupados por op/descrição) ---
spans-agg-heading = Operações de span
spans-col-count = Contagem
spans-col-p50 = p50
spans-col-p95 = p95
spans-col-avg = Média
spans-agg-truncated = A mostrar as { $count } principais operações de span.

# --- Tabela de todos os spans ---
spans-col-span-id = ID do span
spans-col-op = Op
spans-col-description = Descrição
spans-col-timestamp = Data/hora

# --- Paginação (lista de spans) ---
spans-pagination-label = Paginação
spans-pagination-prev = « Anterior
spans-pagination-next = Seguinte »
spans-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}

# --- Detalhe do trace (waterfall) ---
# title-prefix/suffix envolvem o id dinâmico do trace; total/showing-first/of são
# divididos nas fronteiras { $var } da linha de meta.
trace-detail-title-prefix = Trace
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = total
trace-detail-showing-first = a mostrar os primeiros
trace-detail-of = de
trace-detail-empty = Nenhum span encontrado para este trace.
trace-detail-col-span = Span
trace-detail-col-duration = Duração
trace-detail-root-fallback = (raiz do trace)
trace-detail-error-title = erro
trace-detail-span-fallback = span
trace-detail-compressed-note = intervalos inativos comprimidos
trace-detail-gap-title = Intervalo inativo colapsado (sem spans ativos)
trace-detail-lbl-span-id = ID do span
trace-detail-lbl-parent = Span pai
trace-detail-lbl-status = Estado
trace-detail-lbl-start = Deslocamento de início
trace-detail-correlated-errors = Erros correlacionados
trace-detail-col-level = Nível
trace-detail-col-title = Título
trace-detail-col-timestamp = Data/hora
trace-detail-span-count = { $count ->
    [one] { $count } span
   *[other] { $count } spans
}
