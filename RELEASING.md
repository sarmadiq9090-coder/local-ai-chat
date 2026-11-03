# إصدار نسخة جديدة وتوقيعها — دليل المالك

الهدف: لا يقبل المحدِّث داخل Lemon إلا ملفًا وقّعه مفتاحك، فلا يستطيع من يسرق رمزًا (token) أو يعدّل صفحة الإصدارات أن ينشر ملفًا خبيثًا لكل المستخدمين.
لا يُحفظ أي شيء على أجهزتك: التوقيع يجري على GitHub بعد موافقتك (ويمكنك الموافقة من تطبيق GitHub على الجوال)، والمفتاح الاحتياطي يبقى في مدير كلمات المرور فقط.
التفاصيل التقنية بالإنجليزية في [docs/specs/signing.md](docs/specs/signing.md).

> مهم: بعد دمج هذا التغيير لن يُنشر أي إصدار جديد قبل إكمال خطوات الإعداد (1 إلى 4)؛ مهمة `sign` ستتوقف برسالة واضحة.

## الإعداد (مرة واحدة)

### 1. فعّل التحقق بخطوتين (2FA)
من **Settings → Password and authentication → Two-factor authentication** في حسابك على GitHub.

### 2. أنشئ بيئة `release` بموافقتك
في صفحة المستودع: **Settings → Environments → New environment**، واكتب الاسم `release` بالضبط، ثم:
- **Required reviewers**: أضف نفسك. لا تفعّل «Prevent self-review»، لأنك أنت من ينشر وأنت من يوافق.
- **Deployment branches and tags**: اختر **Selected branches and tags**، ثم **Add deployment branch or tag rule**، والنوع **Tag**، والنمط `v*`.

### 3. احمِ وسوم الإصدارات
**Settings → Rules → Rulesets → New ruleset → New tag ruleset**:
- الاسم: `release tags`، و**Enforcement**: Active.
- **Target tags**: Include by pattern ← `v*`.
- القواعد: Restrict creations، Restrict updates، Restrict deletions.
- **Bypass list**: Repository admin (أنت).

يُنصح أيضًا بتفعيل **Release immutability** من **Settings → General → Releases** إن كان الخيار ظاهرًا، فلا يمكن تعديل ملفات الإصدار بعد نشره.

### 4. أنشئ المفاتيح في Codespace (بلا أي جهاز)
1. في صفحة المستودع اضغط **Code → Codespaces → Create codespace on main**. يُفتح محرّر في المتصفح، والخطة المجانية تكفي.
2. في الطرفية (Terminal) أسفل الصفحة:
   ```bash
   sudo apt-get update && sudo apt-get install -y minisign
   cd /tmp
   minisign -G -p minisign-primary.pub -s minisign-primary.key
   minisign -G -p minisign-backup.pub  -s minisign-backup.key
   ```
   كل أمر يطلب كلمة مرور. اختر كلمتين قويتين ومختلفتين.
3. **المفتاح الأساسي يذهب إلى GitHub:**
   ```bash
   cat /tmp/minisign-primary.key
   ```
   انسخ الناتج كاملًا (سطرين)، ثم افتح **Settings → Environments → release → Environment secrets → Add environment secret**:
   - الاسم `MINISIGN_SECRET_KEY` والقيمة ما نسخته.
   - الاسم `MINISIGN_PASSWORD` والقيمة كلمة مرور المفتاح الأساسي.
4. **المفتاح الاحتياطي يذهب إلى مدير كلمات المرور فقط:**
   ```bash
   cat /tmp/minisign-backup.key
   ```
   احفظ الناتج مع كلمة مروره في ملاحظة آمنة في مدير كلمات المرور (السحابي). لا تضعه على GitHub ولا على أي جهاز.
5. **المفاتيح العامة تُرفع إلى المستودع وتُرسَل لنا:**
   ```bash
   cd /workspaces/local-ai-chat
   mkdir -p keys
   cp /tmp/minisign-primary.pub /tmp/minisign-backup.pub keys/
   git checkout -b add-signing-keys
   git add keys/minisign-primary.pub keys/minisign-backup.pub
   git commit -m "Add release-signing public keys"
   git push -u origin add-signing-keys
   cat keys/*.pub
   ```
   افتح طلب الدمج (Pull Request) من الرابط الذي يظهر وادمجه، ثم أرسل لنا ناتج `cat` (الملفان العامان ليسا سرًّا). سنضعهما في البرنامج (`TRUSTED_PUBLIC_KEYS` في `src/update.rs`) في تحديث لاحق. ابتداءً من الإصدار الذي يحتويهما يصبح التوقيع إلزاميًا.
6. **احذف كل شيء:**
   ```bash
   rm -f /tmp/minisign-*.key
   ```
   ثم من https://github.com/codespaces اضغط «…» بجانب الـ Codespace واختر **Delete**.

## كل إصدار

1. أنشئ الوسم `vX.Y.Z` المطابق لـ `version` في `Cargo.toml`، كالمعتاد (أو اطلبه من Claude):
   ```bash
   git tag -a v2.3.0 -m "ملاحظات الإصدار بصيغة Markdown..."
   git push origin v2.3.0
   ```
2. يبني GitHub البرنامج، ثم يتوقف وينتظر موافقتك. يصلك إشعار **Review deployments**، على الجوال أيضًا.
3. افتح التشغيل وتأكد أنه للوسم الذي أنشأته أنت للتو، ثم اضغط **Approve and deploy**. **لا توافق أبدًا على تشغيل لم تبدأه.**
4. بعد ذلك يجري كل شيء تلقائيًا: التوقيع بـ `local-ai-chat vX.Y.Z`، ثم التحقق منه، وشهادة البناء (attestation)، وإنشاء الإصدار كمسودة، والتأكد من وجود الملفات الثلاثة (`local-ai-chat.exe` و`local-ai-chat.exe.sha256` و`local-ai-chat.exe.minisig`)، ثم النشر.

تجربة بلا نشر: **Actions → Release → Run workflow**. توقّع بمفتاح مؤقت وتتحقق، ولا تنشئ أي إصدار.

## إذا ضاع مفتاح أو تسرّب
- **ضاع المفتاح الأساسي أو تسرّب:** ضع المفتاح **الاحتياطي** (من مدير كلمات المرور) وكلمة مروره مكان `MINISIGN_SECRET_KEY` و`MINISIGN_PASSWORD`، وأنشئ مفتاحًا احتياطيًا جديدًا في Codespace، وأخبرنا. الإصدار التالي يُوقَّع بالمفتاح الاحتياطي ويحمل قائمة المفاتيح الجديدة، فتقبله كل النسخ وتتوقف بعده عن الثقة بالمفتاح القديم. إن كان قد تسرّب فغيّر أيضًا كلمة مرور GitHub ورموزه.
- **ضاع المفتاحان معًا:** لا يمكن إصلاح ذلك عن بُعد. يحتاج المستخدمون إلى تنزيل نسخة جديدة يدويًا مرة واحدة من صفحة الإصدارات أو من https://fastkiwi.app/.
