# Changelog

## 0.3.0

### 🇪🇸 Español

- 🧩 **Extensión para el navegador**: PoxiPass ya rellena y guarda contraseñas en Chrome, Edge, Brave, Opera y Vivaldi. Al pinchar en un campo de usuario o contraseña sale un menú con tus cuentas de esa web; un clic y listo.
- 🛡️ **Solo la web de verdad**: la extensión solo ofrece las cuentas del dominio real de la página (`tu-banco.evil.com` no cuela) y nunca rellena nada sin que lo pidas.
- 💾 **Guardar al registrarte o entrar**: cuando envías un formulario con una cuenta nueva, te propone guardarla; si cambiaste la contraseña, te propone actualizarla (la anterior queda en el historial).
- ✨ **Contraseña segura al registrarte**: en los campos de «nueva contraseña» te ofrece una generada con tu configuración y la pone en los dos campos.
- 🔢 **2FA, tarjetas e identidades**: rellena el código 2FA, los datos de la tarjeta en los pagos y tu nombre, email, teléfono y dirección en los formularios.
- 🔐 **Tu contraseña maestra no sale de PoxiPass**: si el llavero está bloqueado, la extensión te manda a desbloquearlo en la app (con Windows Hello o contraseña).
- 🔗 **Cada navegador pide permiso**: la primera vez, PoxiPass te pregunta si lo conectas y enseña el mismo código que la extensión. En **Ajustes → Navegador** ves los conectados y los desconectas cuando quieras.
- ⌨️ **Ctrl+Mayús+L** rellena la web abierta desde el teclado.

### 🇬🇧 English

- 🧩 **Browser extension**: PoxiPass now fills in and saves passwords in Chrome, Edge, Brave, Opera and Vivaldi. Click a username or password field and a menu shows your accounts for that website; one click and you're done.
- 🛡️ **Only the real website**: the extension only offers accounts for the page's real domain (`your-bank.evil.com` doesn't slip through) and never fills anything unless you ask.
- 💾 **Save when you sign up or log in**: when you submit a form with a new account, it offers to save it; if you changed the password, it offers to update it (the old one stays in the history).
- ✨ **Strong password when signing up**: in "new password" fields it offers one generated with your settings and fills both fields.
- 🔢 **2FA, cards and identities**: fills the 2FA code, your card details at checkout and your name, email, phone and address in forms.
- 🔐 **Your master password never leaves PoxiPass**: if the keychain is locked, the extension sends you to unlock it in the app (with Windows Hello or your password).
- 🔗 **Each browser asks for permission**: the first time, PoxiPass asks whether to connect it and shows the same code as the extension. In **Settings → Browser** you see the connected ones and disconnect them whenever you want.
- ⌨️ **Ctrl+Shift+L** fills the open website from the keyboard.

## 0.2.0

### 🇪🇸 Español

- ⚡ **Acceso rápido**: pulsa **Ctrl+Mayús+Espacio** desde cualquier sitio y sale una barra flotante para buscar, justo debajo del ratón. **Enter** escribe la entrada en la ventana donde estabas, **Ctrl+C** copia la contraseña, **Ctrl+B** el usuario y **Ctrl+T** el código 2FA. Si el llavero está bloqueado, lo desbloqueas desde la propia barra.
- 🎯 **Auto-type que acierta**: en Chrome, Edge, Brave, Opera, Vivaldi y Firefox, PoxiPass lee la web que tienes abierta y elige la cuenta por su dominio exacto, no por el título de la pestaña.
- 🔢 **2FA sin buscarlo**: después de escribir usuario y contraseña, el código 2FA queda copiado. Ctrl+V en la siguiente pantalla y listo.
- 🔍 **Búsqueda más lista**: encuentra aunque escribas a medias o con letras salteadas («nfx» → Netflix) y pone arriba lo que más usas.
- 🕘 **Historial del generador**: las últimas contraseñas que copiaste, por si un registro falla. Solo en memoria: se borran al bloquear.
- 🛟 **Recupera un llavero borrado**: si el archivo desaparece (lo borras o lo mueves sin querer), la pantalla de desbloqueo te enseña sus copias y lo restauras con un clic.
- 💾 **Copia extra donde quieras**: en Ajustes → Datos eliges una carpeta más (un USB, Drive, otro disco) y PoxiPass deja ahí también sus copias cifradas. Y desde ahora hay copia desde el momento en que creas el llavero.
- ⚠️ Si un atajo no se puede usar porque lo tiene otra app, ahora siempre te avisa (también en Ajustes).
- 🛡️ **Auto-type anti-phishing**: solo escribe sin preguntar si el dominio real de la web coincide (con la lista oficial de dominios, así `tu-banco.evil.com` o `alguien.github.io` no cuelan). Si solo coincide el título de la ventana, te pide confirmar.
- 🆘 **Kit de emergencia a prueba de despistes**: al generar uno nuevo ya no se puede cerrar sin confirmar que lo tienes, y no se bloquea por inactividad mientras lo apuntas.
- ✍️ **No pierdes lo que escribes**: si sales del editor con cambios sin guardar, te pregunta antes.
- ⇪ **Aviso de Bloq Mayús** al escribir la contraseña maestra.
- 📥 **Importar dos veces no duplica**: las entradas que ya tienes se saltan.
- ⚡ Copiar una contraseña ya no reescribe el llavero entero: va al instante aunque sea enorme.
- 🐛 Los mensajes de error se ven bien («Contraseña incorrecta», etc.) en lugar de «Algo ha fallado».
- 🖱️ **Menú propio con clic derecho** en los campos de texto (Cortar, Copiar, Pegar, Seleccionar todo).
- ↩️ **Deshacer** al mandar una entrada a la papelera.
- 📁 **Carpetas con menú «⋯»** para renombrarlas o borrarlas.
- 📂 **Explorador de archivos mejorado**: escribe o pega una ruta, muévete con las flechas y crea carpetas nuevas.
- ⌨️ **Más teclado**: Enter copia la contraseña de la entrada elegida y empezar a escribir en cualquier sitio te lleva al buscador.
- 📐 **Ventana estrecha bien resuelta**: por debajo de 1100 px la barra lateral se queda solo con iconos.
- ☁️ **A prueba de Drive y OneDrive**: si el llavero cambia en otro PC, PoxiPass lo fusiona entrada a entrada (gana la versión más reciente de cada una) en lugar de pisar nada. Y te avisa cuando llegan cambios.
- 🗄️ **Archivo**: aparta entradas viejas sin borrarlas; no salen en la lista, la búsqueda ni el auto-type.
- 🧹 **Duplicados**: Salud te enseña las entradas repetidas y las fusionas con un clic.
- ✅ **Selección múltiple**: Ctrl+clic y Mayús+clic para elegir varias y moverlas, archivarlas o mandarlas a la papelera de golpe. Ctrl+A elige todas.
- 🫳 **Arrastra entradas** a una carpeta, a Favoritos, al Archivo o a la Papelera.
- 📅 Ordenar también por fecha de creación.
- 🗂️ **Tipos nuevos**: Wi-Fi (con **QR para conectarse**), clave SSH, licencia, cuenta bancaria y documento (DNI, pasaporte…). Y puedes **cambiar el tipo** de cualquier entrada sin perder nada.
- ⏰ **Avisos de caducidad**: Salud te dice qué tarjetas y documentos caducan en menos de 60 días.
- 🔒 **Entradas sensibles**: marca las que quieras (el banco, por ejemplo) y para verlas, copiarlas o escribirlas te pedirá Windows Hello.
- 🎲 **Generador ampliado**: excluye los caracteres que una web no acepte, genera **nombres de usuario** y mira la contraseña en **letra grande** o como **QR** para pasarla a la tele o al móvil.
- 👁️ **Ver adjuntos sin guardarlos** (imágenes y texto): nada se escribe en disco.
- 📝 **Notas con formato**: títulos, listas, negrita y enlaces (Markdown).

### 🇬🇧 English

- ⚡ **Quick access**: press **Ctrl+Shift+Space** anywhere for a floating search bar, right below your mouse. **Enter** types the entry into the window you were in, **Ctrl+C** copies the password, **Ctrl+B** the username and **Ctrl+T** the 2FA code. If the keychain is locked, you unlock it right from the bar.
- 🎯 **Auto-type that gets it right**: in Chrome, Edge, Brave, Opera, Vivaldi and Firefox, PoxiPass reads the website you have open and picks the account by its exact domain, not by the tab title.
- 🔢 **2FA without looking for it**: after typing your username and password, the 2FA code is already copied. Ctrl+V on the next screen and you're in.
- 🔍 **Smarter search**: finds things even with half-typed or skipped letters ("nfx" → Netflix) and puts what you use most at the top.
- 🕘 **Generator history**: the last passwords you copied, in case a sign-up fails. Memory only: cleared when you lock.
- 🛟 **Recover a deleted keychain**: if the file disappears (deleted or moved by mistake), the unlock screen shows its backups and you restore it in one click.
- 💾 **Extra backup wherever you want**: in Settings → Data pick one more folder (a USB drive, Drive, another disk) and PoxiPass keeps its encrypted backups there too. And there's now a backup from the moment you create the keychain.
- ⚠️ If a shortcut can't be used because another app has it, you're now always told (in Settings too).
- 🛡️ **Anti-phishing auto-type**: it only types without asking when the website's real domain matches (using the official domain list, so `your-bank.evil.com` or `someone.github.io` don't slip through). If only the window title matches, it asks you to confirm.
- 🆘 **Slip-proof emergency kit**: when you generate a new one it can't be closed until you confirm you have it, and it won't lock for inactivity while you write it down.
- ✍️ **Don't lose what you type**: leaving the editor with unsaved changes asks first.
- ⇪ **Caps Lock warning** when typing your master password.
- 📥 **Importing twice doesn't duplicate**: entries you already have are skipped.
- ⚡ Copying a password no longer rewrites the whole keychain: instant even for huge ones.
- 🐛 Error messages now show properly ("Wrong password", etc.) instead of "Something went wrong".
- 🖱️ **Own right-click menu** in text fields (Cut, Copy, Paste, Select all).
- ↩️ **Undo** when moving an entry to the trash.
- 📁 **Folders with a «⋯» menu** to rename or delete them.
- 📂 **Better file explorer**: type or paste a path, move with the arrow keys and create new folders.
- ⌨️ **More keyboard**: Enter copies the selected entry's password, and typing anywhere jumps to search.
- 📐 **Narrow window done right**: below 1100 px the sidebar collapses to icons.
- ☁️ **Drive- and OneDrive-proof**: if the keychain changes on another PC, PoxiPass merges it entry by entry (the newest version of each wins) instead of overwriting anything. And it tells you when changes arrive.
- 🗄️ **Archive**: set old entries aside without deleting them; they're hidden from the list, search and auto-type.
- 🧹 **Duplicates**: Health shows repeated entries and you merge them in one click.
- ✅ **Multi-select**: Ctrl+click and Shift+click to pick several and move, archive or trash them at once. Ctrl+A selects all.
- 🫳 **Drag entries** onto a folder, Favorites, Archive or Trash.
- 📅 Sort by creation date too.
- 🗂️ **New types**: Wi-Fi (with a **QR code to connect**), SSH key, license, bank account and document (ID, passport…). And you can **change the type** of any entry without losing anything.
- ⏰ **Expiry alerts**: Health tells you which cards and documents expire within 60 days.
- 🔒 **Sensitive entries**: mark the ones you want (your bank, say) and viewing, copying or typing them will ask for Windows Hello.
- 🎲 **Bigger generator**: exclude characters a website won't accept, generate **usernames**, and view the password in **large type** or as a **QR code** to pass it to a TV or phone.
- 👁️ **View attachments without saving them** (images and text): nothing is written to disk.
- 📝 **Formatted notes**: headings, lists, bold and links (Markdown).

## 0.1.0

### 🇪🇸 Español

- 🎉 **Primera versión**: PoxiPass guarda tus contraseñas cifradas en un archivo de tu PC. Sin nubes, sin cuentas y sin telemetría.
- 🔐 **Cifrado serio**: Argon2id para tu contraseña maestra y XChaCha20-Poly1305 para los datos. Con el llavero abierto, las contraseñas siguen selladas en memoria y solo se descifran al verlas, copiarlas o escribirlas.
- 🆘 **Kit de emergencia**: si olvidas la contraseña maestra, unas palabras de la **Lista Poxi** (castiza de verdad) te devuelven el acceso. Se guarda en un PDF con el diseño de la app.
- 🎲 **Generador** de contraseñas y de frases fáciles de recordar, con medidor de fortaleza y tiempo estimado para crackearlas.
- ⚡ **Alta rápida**: escribe la web o el nombre, el usuario, y la contraseña ya viene generada.
- 🔢 **Códigos 2FA** dentro de cada entrada: pega la clave o una captura del QR.
- ⌨️ **Auto-type**: pulsa Ctrl+Alt+A en cualquier login y PoxiPass escribe usuario y contraseña por ti.
- 🩺 **Salud del llavero**: contraseñas débiles, repetidas, antiguas y **filtradas** (Have I Been Pwned, sin que tu contraseña salga del PC).
- 👆 **Windows Hello**: desbloquea con huella, cara o PIN.
- 📋 **Portapapeles seguro**: lo que copias se borra solo y nunca entra en el historial de Win+V.
- 💳 **Tarjetas, notas seguras e identidades**, con adjuntos cifrados e historial de cambios.
- 📥 **Importa** desde Chrome, Edge, Firefox, Bitwarden, KeePass y KeePassXC.
- 💾 **Copias automáticas** cifradas, restaurables desde la propia app.
- 🎨 **Temas** oscuro y claro, 6 colores y todo en español e inglés.

### 🇬🇧 English

- 🎉 **First release**: PoxiPass keeps your passwords encrypted in a file on your PC. No cloud, no accounts, no telemetry.
- 🔐 **Serious encryption**: Argon2id for your master password and XChaCha20-Poly1305 for your data. While the keychain is open, passwords stay sealed in memory and are only decrypted to show, copy or type them.
- 🆘 **Emergency kit**: if you forget your master password, a few words from the **Poxi List** get you back in. Saved as a PDF with the app's own design.
- 🎲 **Generator** for passwords and easy-to-remember passphrases, with a strength meter and estimated cracking time.
- ⚡ **Quick add**: type the website or a name and the username; the password is already generated.
- 🔢 **2FA codes** inside each entry: paste the key or a screenshot of the QR code.
- ⌨️ **Auto-type**: press Ctrl+Alt+A on any login page and PoxiPass types your username and password for you.
- 🩺 **Keychain health**: weak, reused, old and **breached** passwords (Have I Been Pwned, without your password ever leaving your PC).
- 👆 **Windows Hello**: unlock with your fingerprint, face or PIN.
- 📋 **Safe clipboard**: whatever you copy clears itself and never lands in the Win+V history.
- 💳 **Cards, secure notes and identities**, with encrypted attachments and change history.
- 📥 **Import** from Chrome, Edge, Firefox, Bitwarden, KeePass and KeePassXC.
- 💾 **Automatic encrypted backups**, restorable from inside the app.
- 🎨 **Dark and light themes**, 6 colors, all in Spanish and English.
