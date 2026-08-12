// Issue-detail controls: stack-frame filter/order/copy, breadcrumb type filter.
//
// Purely presentational — every frame and crumb is already rendered server-side,
// so these hide or reorder rows rather than fetching anything. Without JS the
// page still renders in full, just without the controls doing anything.
(function () {
    Array.prototype.forEach.call(document.querySelectorAll('.frame-list'), function (list) {
        var controls = list.previousElementSibling;
        if (!controls || !controls.classList.contains('frame-controls')) return;

        var inAppOnly = controls.querySelector('[data-in-app-only]');
        if (inAppOnly) {
            inAppOnly.addEventListener('change', function () {
                list.classList.toggle('in-app-only', inAppOnly.checked);
            });
        }

        var reverse = controls.querySelector('[data-reverse-frames]');
        if (reverse) {
            reverse.addEventListener('click', function () {
                var kids = Array.prototype.slice.call(list.children);
                kids.reverse().forEach(function (k) { list.appendChild(k); });
                list.classList.toggle('frames-reversed');
            });
        }
    });

    var crumbFilter = document.querySelector('[data-crumb-filter]');
    if (crumbFilter) {
        var rows = document.querySelectorAll('.breadcrumb-table tbody tr');
        crumbFilter.addEventListener('change', function () {
            var want = crumbFilter.value;
            Array.prototype.forEach.call(rows, function (tr) {
                var cat = tr.getAttribute('data-category') || '';
                tr.style.display = (!want || cat === want) ? '' : 'none';
            });
        });
    }

    // Delegated so frames inside a collapsed library run are covered too.
    document.addEventListener('click', function (ev) {
        var btn = ev.target.closest ? ev.target.closest('.frame-copy') : null;
        if (!btn) return;
        ev.preventDefault();
        var text = btn.getAttribute('data-copy') || '';
        if (!navigator.clipboard) return;
        navigator.clipboard.writeText(text).then(function () {
            var was = btn.textContent;
            btn.textContent = '✓';
            setTimeout(function () { btn.textContent = was; }, 1200);
        });
    });
})();
