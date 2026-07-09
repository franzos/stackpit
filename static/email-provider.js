// Add-email form: SMTP is driven by the server's instance [email] provider, so
// hide the per-provider API token field (and drop its `required`) when SMTP is selected.
(function () {
    var sel = document.getElementById('int_provider');
    var tokenField = document.getElementById('int_token_field');
    var tokenInput = document.getElementById('int_secret');
    var hint = document.getElementById('int_smtp_hint');
    if (!sel) return;
    function sync() {
        var isSmtp = sel.value === 'smtp';
        if (tokenField) tokenField.hidden = isSmtp;
        if (tokenInput) tokenInput.disabled = isSmtp;
        if (hint) hint.hidden = !isSmtp;
    }
    sel.addEventListener('change', sync);
    sync();
})();
