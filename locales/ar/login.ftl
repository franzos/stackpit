# صفحة تسجيل الدخول المستقلة (templates/login.html) بالإضافة إلى نصوص لافتات
# OAuth/تسجيل الخروج المنتجة في src/html/login.rs. يحمل login-token-help
# ترميز <code> مضمّنًا ويُعرض باستخدام |safe.
login-page-title = تسجيل الدخول — Stackpit
login-welcome = مرحبًا بعودتك
login-subtitle = سجّل الدخول لإدارة تتبّع الأخطاء الخاص بك
login-sso = تسجيل الدخول عبر SSO
login-or = أو
login-token-label = رمز المسؤول
login-token-placeholder = أدخل رمزك الرئيسي…
login-submit = تسجيل الدخول
login-token-help = يأتي رمز المسؤول من <code class="text-mono">admin_token</code> في <code class="text-mono">stackpit.toml</code>. حرّر الملف وأعد تشغيل <code class="text-mono">stackpit serve</code> لتطبيق التغييرات.
login-docs = التوثيق
login-selfhosting = دليل الاستضافة الذاتية

# لافتة الخطأ (مشتقّة من رموز OAuth ‎?error=‎) ولافتة معلومات تسجيل الخروج.
login-error-state-mismatch = تم العبث بجلسة تسجيل دخولك أو انتهت صلاحيتها. يرجى المحاولة مرة أخرى.
login-error-session-expired = انتهت صلاحية جلستك. يرجى تسجيل الدخول مرة أخرى.
login-error-missing-response = أعاد مزوّد الهوية الخاص بك استجابة غير مكتملة. يرجى المحاولة مرة أخرى.
login-error-token-exchange = تعذّر علينا إكمال تسجيل الدخول عبر مزوّد الهوية الخاص بك. يرجى المحاولة بعد لحظات.
login-error-provisioning = تعذّر إنشاء حسابك. تواصل مع المسؤول.
login-error-email-conflict = يوجد حساب بهذا البريد الإلكتروني بالفعل. تواصل مع المسؤول.
login-error-session-unavailable = تسجيل الدخول غير متاح مؤقتًا. يرجى المحاولة بعد لحظات.
login-error-encryption = تسجيل الدخول مُهيّأ بشكل خاطئ في هذا النشر. تواصل مع المسؤول.
login-error-generic = فشل تسجيل الدخول. يرجى المحاولة مرة أخرى.
login-error-invalid-token = رمز غير صالح
login-logout-local = تم تسجيل الخروج من Stackpit. لم تُنهَ جلستك لدى مزوّد الهوية -- سجّل الخروج هناك بشكل منفصل إن لزم الأمر.
