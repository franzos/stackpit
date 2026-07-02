# Interface de métricas: a lista de métricas por projeto e a página de detalhe da
# série da métrica. Reutiliza nav-metrics. As strings com contagem usam plurais
# tv_count ([one]/[other]).

# --- Sufixo do título da página ---
metrics-title-suffix = — Stackpit

# --- Lista de métricas ---
metrics-list-empty = Nenhuma métrica encontrada. Os eventos de métrica aparecerão aqui assim que forem recebidos.
metrics-col-mri = MRI
metrics-col-type = Tipo
metrics-col-data-points = Pontos de dados
metrics-col-first-seen = Visto pela primeira vez
metrics-col-last-seen = Visto pela última vez

# --- Paginação ---
metrics-pagination-label = Paginação
metrics-pagination-prev = « Anterior
metrics-pagination-next = Seguinte »
metrics-count = { $count ->
    [one] { $count } métrica
   *[other] { $count } métricas
}

# --- Detalhe da métrica (intervalos horários) ---
metrics-detail-empty = Nenhum ponto de dados no intervalo de tempo selecionado.
metrics-detail-col-time = Hora (intervalo horário)
metrics-detail-col-count = Contagem
metrics-detail-col-sum = Soma
metrics-detail-col-min = Mín
metrics-detail-col-max = Máx
metrics-detail-col-avg = Média
