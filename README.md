<div align="center">

# Tiling Window

### Gestor de ventanas de mosaico para macOS, escrito en Rust

Gestión automática de ventanas nativa, ligera y basada en eventos.

[![Rust](https://img.shields.io/badge/Rust-edition%202024-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/macOS-11.0%2B-000000?logo=apple&logoColor=white)](https://www.apple.com/macos/)
[![Licencia](https://img.shields.io/badge/licencia-por%20definir-lightgrey)](#-licencia)
[![Especificaciones](https://img.shields.io/badge/SDD-spec--driven-blue)](specs/README.md)

</div>

---

`tiling_window` descubre las ventanas de las aplicaciones a través de la API de
Accesibilidad de macOS, las organiza con un motor de *layout* independiente de la
plataforma y mantiene cada pantalla en mosaico automáticamente cuando las
ventanas se abren, cierran, mueven o cambian de estado — sin sondeo, sin demonios
en segundo plano y sin APIs privadas de Apple.

El proyecto se construye de forma incremental bajo un flujo de **Desarrollo
Guiado por Especificaciones (SDD)**. Las especificaciones son la fuente de verdad
del comportamiento; consulta [`AGENTS.md`](AGENTS.md) para las convenciones de
ingeniería y [`specs/`](specs/README.md) para el índice completo.

## Características

| Característica | Descripción |
| --- | --- |
| **Integración nativa con macOS** | Solo APIs públicas de Accesibilidad, Core Graphics y AppKit. Sin frameworks privados. |
| **Mosaico automático** | El gestor vuelve a organizar cada pantalla afectada cuando cambian ventanas o aplicaciones, con actualizaciones *debounced*, idempotentes y convergentes. |
| **Múltiples diseños** | Master/Stack, columnas, filas, monocle y quad (2x2). |
| **Modo mover con imán** | Un atajo activa el modo mover: las flechas llevan la ventana enfocada a la mitad de pantalla elegida y repelen a la ventana de abajo al lado opuesto. |
| **Pantalla completa** | Alterna el fullscreen nativo de macOS de la ventana enfocada. |
| **Estado por pantalla** | Cada pantalla activa conserva su propio orden de mosaico y configuración de diseño durante la vida del proceso. |
| **Atajos globales** | Teclas de acceso rápido funcionales y una superposición translúcida de atajos. |
| **Basado en eventos** | Un bucle en primer plano consume eventos normalizados de ventanas y aplicaciones; sin hilos, temporizadores ni runtime asíncrono. |
| **Núcleo independiente** | Los algoritmos de diseño y el controlador de mosaico no contienen tipos de macOS y son totalmente testeables. |

## Requisitos

- **macOS 11.0** o superior.
- Toolchain estable de **Rust** (edition 2024).
- **Permiso de Accesibilidad** concedido al binario en ejecución o al paquete
  `.app` instalado.

## Compilación

```sh
cargo build --release
```

El binario de release se genera en `target/release/tiling_window`.

## Instalación como agente de inicio de sesión (opcional)

`scripts/install-agent.sh` empaqueta el binario de release como un paquete
`TilingWindow.app` firmado e instala un `LaunchAgent` por usuario para que se
inicie al iniciar sesión:

```sh
./scripts/install-agent.sh
```

El script compila el binario si es necesario, firma el paquete con una identidad
autofirmada estable (para que el permiso de Accesibilidad sobreviva a las
recompilaciones) y registra el agente en `~/Library/LaunchAgents`. El material de
firma se guarda fuera del repositorio, en
`~/Library/Application Support/tiling-window`.

Para eliminarlo:

```sh
./scripts/uninstall-agent.sh
```

## Permiso de Accesibilidad

La inspección, el movimiento y la observación de ventanas requieren acceso de
Accesibilidad. El comando `permissions` informa el estado actual e imprime
instrucciones:

```sh
cargo run -- permissions
```

Concédelo manualmente en **Ajustes del Sistema → Privacidad y seguridad →
Accesibilidad**, activando la entrada del programa (o añadiéndola con `+`).
Después de una recompilación es posible que debas desactivar y volver a activar
la entrada.

## Uso

```sh
cargo run -- <comando> [opciones]
```

Ejecuta `cargo run -- --help` (o sin argumentos) para imprimir la referencia
completa de comandos y selectores.

### Comandos

| Comando | Descripción |
| --- | --- |
| `permissions` | Informa el estado de autorización de Accesibilidad. |
| `windows` | Lista las ventanas accesibles de las aplicaciones en ejecución. |
| `move` | Mueve/redimensiona una ventana a una geometría absoluta o predefinida. |
| `focus` | Enfoca (eleva) una ventana. |
| `displays` | Lista las pantallas activas y su geometría. |
| `display-info <id>` | Muestra la geometría detallada de una pantalla. |
| `layout` | Previsualiza un diseño de mosaico sin cambiar ventanas. |
| `apply <layout>` | Aplica un diseño de mosaico a las ventanas de una pantalla. |
| `watch` | Monitoriza eventos de ventanas hasta interrumpir (Ctrl+C). |
| `overlay` | Muestra una superposición translúcida de atajos (alternar: `⌘⌥-`). |
| `run [--no-auto]` | Gestor de ventanas en primer plano: overlay, atajos y mosaico automático. |
| `-h`, `--help` | Muestra la ayuda. |

### Selectores de ventana

Los selectores usan `--focused` por defecto.

| Selector | Descripción |
| --- | --- |
| `--focused` | La ventana enfocada de la aplicación en primer plano. |
| `--pid <PID>` | Restringe a un proceso. |
| `--app <NAME>` | Restringe a un nombre de aplicación. |
| `--title <SUBSTRING>` | Restringe a una subcadena del título. |
| `--index <N>` | Elige la N-ésima coincidencia en un orden determinista. |

### Geometría de `move`

```sh
cargo run -- move --x <f64> --y <f64> --width <f64> --height <f64>
cargo run -- move --to <left|right|maximize|center>
```

### Opciones de `layout` y `apply`

```sh
cargo run -- layout --layout <masterstack|columns|rows|monocle|quad> [--display <id>] \
    [--outer-gap <f64>] [--inner-gap <f64>] [--ratio <f64>] [--master-count <usize>]

cargo run -- apply <masterstack|columns|rows|monocle|quad> [--display <id>] [--dry-run]
```

### Gestor en primer plano

```sh
cargo run -- run           # overlay + atajos + mosaico automático (por defecto)
cargo run -- run --no-auto # solo overlay + atajos (sin diseño automático)
```

`run` permanece en primer plano hasta Ctrl+C, bombeando eventos de AppKit,
drenando acciones de atajos y eventos de observadores, y reorganizando las
pantallas que el controlador reporta como listas. Todos los demás comandos
siguen funcionando de forma independiente.

## Atajos de teclado

Los atajos se definen en una única tabla de enlaces (`src/core/shortcut.rs`) que
alimenta tanto la referencia del overlay como los atajos globales registrados, de
modo que la lista mostrada y las acciones ejecutadas nunca divergen.

| Atajo | Acción |
| --- | --- |
| `⌘⌥-` | Mostrar u ocultar el overlay |
| `⌘⌥S` | Diseño Master/Stack |
| `⌘⌥C` | Diseño en columnas |
| `⌘⌥R` | Diseño en filas |
| `⌘⌥Q` | Diseño quad (2x2) |
| `⌘⌥M` | Diseño monocle |
| `⌘⌥Espacio` | Entrar o salir del modo mover |
| `⌘⌥F` | Pantalla completa de la ventana enfocada |
| `⌘⌥←/→/↑/↓` | Mover el foco (o la ventana, en modo mover) |
| `⌘⌥⇧←/→/↑/↓` | Mover la ventana enfocada |

## Diseños

| Diseño | Comportamiento |
| --- | --- |
| `masterstack` | Una columna maestra a la izquierda más una pila a la derecha. |
| `columns` | Columnas verticales de ancho igual. |
| `rows` | Filas horizontales de alto igual. |
| `monocle` | Todas las ventanas comparten el área útil completa. |
| `quad` | Dos columnas: la primera mitad de ventanas a la izquierda y la segunda a la derecha, cada una apilada. Con 4 ventanas queda un 2x2. |

La `LayoutConfig` por defecto es `masterstack` con `outer_gap = 8.0`,
`inner_gap = 8.0`, `master_ratio = 0.55` y `master_count = 1`.

## Modo mover y pantalla completa

`⌘⌥Espacio` alterna el **modo mover**. Mientras está activo, las flechas
`⌘⌥←/→/↑/↓` mueven la ventana enfocada a la mitad de pantalla indicada
(imán) en lugar de mover el foco:

- La ventana enfocada pasa a ocupar toda la mitad hacia la que se mueve.
- Si otra ventana ya ocupaba esa mitad, se **repela** a la mitad contraria:
  con una sola ventana invadida se refleja hacia el lado opuesto conservando
  su tamaño; con varias se apilan para llenar la mitad contraria.
- El overlay se muestra al entrar en el modo y se restaura su visibilidad
  anterior al salir.

Con el mosaico automático activo, el intercambio también se refleja en el
orden de tiling, de modo que un retile posterior respeta la nueva posición.
Con `--no-auto`, el cambio es solo de geometría.

`⌘⌥F` alterna el **pantalla completa nativa** de macOS (`AXFullScreen`) de la
ventana enfocada. Las ventanas en pantalla completa siguen excluidas del
mosaico; al salir vuelven a ser gestionables.

## Cómo funciona

1. **Descubrimiento** — las ventanas accesibles y controlables se descubren a
   través de la API de Accesibilidad junto con una identidad de runtime estable
   (`WindowKey` = id de proceso + id de ventana de runtime).
2. **Sistema de eventos** — los observadores normalizan los cambios de ciclo de
   vida de aplicaciones y ventanas en eventos de dominio independientes de la
   plataforma y mantienen un registro de runtime. Los eventos de geometría se
   fusionan.
3. **Controlador de mosaico** — se siembra un orden de mosaico por pantalla al
   arrancar y se mantiene estable: las ventanas nuevas se añaden al final, las
   destruidas se eliminan y solo un reordenamiento explícito cambia el orden. Los
   cambios de título, geometría y foco nunca lo reordenan.
4. **Reorganización con debounce** — las pantallas sucias se reorganizan tras un
   intervalo de asentamiento (150 ms por defecto). Los cambios de geometría
   programáticos se comparan con la geometría que macOS aplicó realmente y se
   suprimen, lo que evita bucles de retroalimentación y garantiza la convergencia
   en ventanas restringidas.
5. **Motor de diseño** — algoritmos puros e independientes de la plataforma
   convierten un área de trabajo más una colección ordenada de ventanas en
   rectángulos objetivo deterministas.
6. **Ejecución** — los rectángulos objetivo se aplican a través de la API de
   Accesibilidad; las ventanas que ya están dentro de la tolerancia de geometría
   se omiten.

## Arquitectura

```
src/
├── main.rs                 # Punto de entrada, despacho de comandos, códigos de salida
├── app/                    # Gestor en primer plano, controlador de mosaico, overlay
│   ├── tiling.rs           # Controlador de mosaico automático / política de suciedad
│   ├── event_processor.rs  # Normalización y fusión de eventos
│   ├── registry.rs         # Registro de ventanas de runtime
│   ├── layout_planner.rs   # Construye planes de diseño a partir del estado
│   ├── layout_executor.rs  # Aplica planes, registra la geometría esperada
│   ├── overlay.rs          # Contenido/estado del overlay
│   └── ...
├── platform/macos/         # Todo el código de AppKit / Carbon / Accesibilidad
│   ├── accessibility.rs    # Comprobación de permisos, helpers de AX
│   ├── applications.rs     # Helpers de NSRunningApplication
│   ├── displays.rs         # Descubrimiento y geometría de pantallas
│   ├── windows.rs          # Descubrimiento de ventanas
│   ├── control.rs          # Mover/redimensionar ventanas
│   ├── observers.rs        # Observadores de eventos nativos
│   ├── hotkey.rs           # Registro de atajos globales
│   ├── overlay.rs          # NSWindow del overlay
│   └── core_graphics.rs    # Enlaces de Core Graphics
├── core/                   # Modelos de dominio independientes de la plataforma
│   ├── action.rs           # Acciones y direcciones
│   ├── coordinates.rs      # Sistemas de coordenadas
│   ├── display*.rs         # Modelo y asignación de pantallas
│   ├── event.rs            # Eventos de dominio y WindowKey
│   ├── geometry.rs         # Primitivas Rect / Size
│   ├── layout*.rs          # Config de diseño, plan, informe de ejecución
│   ├── movement.rs         # Movimiento por zonas con repelencia
│   ├── navigation.rs       # Búsqueda direccional de vecinos
│   ├── selector.rs         # Selección y ordenación de ventanas
│   ├── shortcut.rs         # Tabla de enlaces (overlay + atajos)
│   └── window.rs           # Identificadores y metadatos de ventanas
├── layouts/                # Algoritmos de diseño puros
│   ├── master_stack.rs
│   ├── columns.rs
│   ├── rows.rs
│   ├── quad.rs
│   └── monocle.rs
├── commands/               # Parseo y despacho de CLI
├── error.rs                # Tipos de error de la aplicación
└── ...
```

**Reglas arquitectónicas**

- `core` nunca depende de APIs de macOS; los algoritmos de diseño operan sobre
  geometría independiente de la plataforma.
- Todo el código de AppKit, Carbon y Accesibilidad permanece dentro de
  `platform/macos`, tras abstracciones seguras de Rust.
- Los comandos deciden *qué* hacer; la capa de plataforma realiza el trabajo
  nativo.
- Las llamadas a AppKit/Carbon/AX ocurren solo en el hilo principal; sin
  `Send`/`Sync` manuales para handles nativos.

## Desarrollo

Antes de considerar un cambio finalizado, ejecuta:

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

Para validación de release:

```sh
cargo build --release
```

Toda la lógica independiente de la plataforma (geometría, diseños, orden y
política de mosaico, selectores, validación de configuración) está cubierta por
pruebas unitarias que no requieren permisos de Accesibilidad.

## Especificaciones

El comportamiento de cada funcionalidad está definido por especificación. Lee la
especificación relevante completa antes de implementar.

| Spec | Título | Estado |
| --- | --- | --- |
| [SPEC-001](specs/001-windows-discovery.md) | Descubrimiento de ventanas | Borrador |
| [SPEC-002](specs/002-native-macos-window-control.md) | Control nativo de ventanas en macOS | Borrador |
| [SPEC-003](specs/003-native-macos-display-management.md) | Gestión nativa de pantallas en macOS | Borrador |
| [SPEC-004](specs/004-platform-independent-layout-engine.md) | Motor de diseño independiente de la plataforma | Borrador |
| [SPEC-005](specs/005-native-layout.md) | Aplicación nativa del diseño | Borrador |
| [SPEC-006](specs/006-windows-event-system.md) | Sistema de eventos de ventanas | Aprobada |
| [SPEC-007](specs/007-shortcut-overlay.md) | Ventana de superposición de atajos | Aprobada |
| [SPEC-008](specs/008-shortcut-actions.md) | Acciones de atajos | Implementada |
| [SPEC-009](specs/009-automatic-tiling.md) | Mosaico automático | Implementada |

Consulta [`specs/README.md`](specs/README.md) para notas de numeración y
sustitución.

## Limitaciones

- macOS no expone una gestión de ventanas sin restricciones a través de APIs
  públicas. Algunas aplicaciones no admiten redimensionamiento programático y las
  ventanas del sistema pueden no ser gestionables.
- Las ventanas en pantalla completa requieren un manejo especial y no se
  organizan en mosaico.
- El comportamiento multi-Space está limitado por las APIs públicas de macOS; los
  Spaces no se gestionan.
- El permiso de Accesibilidad debe concederse explícitamente y puede perderse al
  recompilar el binario (vuelve a concederlo o usa el paquete `.app` firmado).
- El orden de mosaico y el estado del diseño son solo de runtime y no persisten
  entre invocaciones de `run`.

## Roadmap

- **SPEC-010** — Ventanas flotantes y reglas por aplicación (aún sin definir).
- Diseño BSP, movimiento de ventanas entre pantallas, archivos de configuración
  y mejoras de animación (ver SPEC-009 §15, *Out of Scope*).

## Licencia

Este proyecto aún no incluye una licencia explícita. Hasta que se defina una,
todos los derechos quedan reservados por el autor.

<div align="center">

Hecho con Rust para macOS

</div>
