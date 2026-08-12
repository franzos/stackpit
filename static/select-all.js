// "Select all N matching this filter" gate for the all-matching bulk forms.
//
// These forms carry no row checkboxes: they act on a server-computed filter
// match, so there is nothing to tick and the row-checkbox bar (bulk.js) cannot
// gate them. The affirmation *is* the selection. Buttons ship `disabled` in the
// markup, so without JS the action stays unavailable rather than one-click.
(function () {
    var gates = document.querySelectorAll('.select-all-gate');
    if (!gates.length) return;
    Array.prototype.forEach.call(gates, function (gate) {
        var form = gate.closest('form');
        if (!form) return;
        var box = gate.querySelector('input[type="checkbox"]');
        var buttons = form.querySelectorAll('button[type="submit"]');
        if (!box || !buttons.length) return;
        function upd() {
            Array.prototype.forEach.call(buttons, function (b) {
                b.disabled = !box.checked;
            });
        }
        box.addEventListener('change', upd);
        upd();
    });
})();
