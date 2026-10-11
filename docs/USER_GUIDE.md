# Guía de usuario de HollyShow

HollyShow proyecta letras, versículos y medios durante el culto, y se controla desde el computador o desde un celular en la misma red.

## 1. Instalación

- **Windows 10/11 (64 bits):** descarga el instalador `.exe` (o `.msi`) desde la página de Releases. Si aparece SmartScreen, pulsa «Más información» > «Ejecutar de todas formas» (mientras la app no esté firmada). El instalador incluye WebView2.
- **macOS 12 o superior:** abre el `.dmg` y arrastra HollyShow a Aplicaciones. Si dice «desarrollador no identificado», haz clic derecho sobre la app > Abrir > Abrir.
- **Linux:** instala el `.deb` (`sudo apt install ./hollyshow_*.deb`) o da permisos al `.AppImage` (`chmod +x`) y ejecútalo.

## 2. Primer inicio

1. Abre HollyShow: se muestra la ventana del operador.
2. Elige el idioma (Español por defecto) en Ajustes.
3. Crea o importa canciones y elige las versiones de la Biblia (se incluyen textos de dominio público; las traducciones con derechos de autor debes importarlas con tu licencia).
4. Arma el orden del culto, o parte de una plantilla de servicio.

## 3. Conectar el proyector

1. Conecta el proyector o pantalla por HDMI y configúralo como pantalla extendida (no duplicada) en el sistema.
2. En Ajustes > Salidas, asigna la pantalla a la salida principal; puedes añadir una segunda salida (por ejemplo el stage display para el escenario).
3. Usa «Probar salida» para ver el patrón; el modo culto oculta distracciones del operador durante el servicio.
4. Si la salida aparece en la pantalla equivocada, reasígnala en Ajustes > Salidas.

## 4. Emparejar el celular (control remoto) con QR

1. Conecta el computador y el celular a la **misma red Wi-Fi**.
2. En HollyShow abre Remoto > «Emparejar dispositivo» y elige el rol (operador, pastor, músico o congregación).
3. Escanea el código QR con la cámara del celular: se abre la página del remoto en el navegador. No hay que instalar nada.
4. El código es de un solo uso y caduca; genera otro para cada dispositivo. Puedes revocar dispositivos desde la misma pantalla.

## 5. Avisos del firewall

- **Windows:** al primer inicio aparece «Windows Defender Firewall ha bloqueado algunas características». Marca **Redes privadas** y pulsa «Permitir acceso». Si la red de la iglesia está como «Pública», cámbiala a «Privada» o crea la regla en Seguridad de Windows > Firewall.
- **macOS:** aparece «¿Quieres que la aplicación HollyShow acepte conexiones de red entrantes?». Pulsa **Permitir**. Si lo negaste, ve a Ajustes del Sistema > Red > Firewall > Opciones y permite HollyShow.
- **Linux:** si usas `ufw`, permite el puerto que muestra Remoto > Diagnóstico (`sudo ufw allow <puerto>/tcp`).

## 6. Solución de problemas

**El celular no abre la página o se desconecta**

- Verifica que ambos estén en la misma red y subred; abre Remoto > Diagnóstico para ver la dirección y puerto que usa la app.
- **Aislamiento de clientes («Client/AP isolation»):** muchos Wi-Fi de iglesias aíslan los dispositivos entre sí para que no se vean. Síntomas: el celular tiene internet pero no alcanza al computador. Soluciones: desactiva el aislamiento en el router/AP o usa una red/VLAN para producción; conecta el computador por cable al mismo router; o crea un router propio dedicado al audiovisual (lo más fiable); o comparte datos móviles desde el celular del operador al computador como último recurso.
- Redes de invitados y de empresa suelen bloquear la comunicación entre dispositivos; usa una red privada.
- Las VPN activas en el computador o el celular pueden desviar el tráfico: desactívalas.
- Si cambió la IP del computador, vuelve a escanear el QR.

**La salida se ve en la pantalla equivocada o con otra resolución:** revisa Ajustes > Salidas y la configuración de pantallas del sistema (extendida, no duplicada).

**No hay sonido o video:** revisa los códecs soportados por el sistema y que el archivo sea accesible; usa formatos comunes (MP4/H.264, MP3, WebM).

**Reportar un problema:** abre Remoto > Diagnóstico, copia la información y adjúntala al issue en GitHub.
