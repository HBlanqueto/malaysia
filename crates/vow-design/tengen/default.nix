# Tengen: la única fuente de tema y diseño de Keel.
#
# tokens.json y components.json son los datos (neutros, sin framework). Este
# archivo los traduce a Slint, CSS y QML. Usa únicamente builtins, así que se
# evalúa sin nixpkgs:
#
#   nix eval --raw -f crates/vow-design/tengen slint > generated/tengen.slint
#   nix eval --raw -f crates/vow-design/tengen css   > generated/tengen.css
#   nix eval --raw -f crates/vow-design/tengen qml   > generated/Tengen.qml
#
# Desde un módulo: (import ./crates/vow-design/tengen).slint, o `files pkgs` para una
# derivación con los archivos generados.
let
  tokens = builtins.fromJSON (builtins.readFile ./tokens.json);
  components = builtins.fromJSON (builtins.readFile ./components.json);

  themes = [
    "light"
    "dark"
    "dark-tint"
  ];
  inherit (builtins)
    attrNames
    concatStringsSep
    elemAt
    isAttrs
    isFloat
    isInt
    isList
    isString
    match
    stringLength
    substring
    toString
    ;
  colorNames = attrNames tokens.themes.light;
  lines = f: xs: concatStringsSep "\n" (map f xs);
  num = v: if isFloat v then toString v else toString v;
  ident = s: builtins.replaceStrings [ "-" ] [ "_" ] s;
  firstFamily = f: let parts = builtins.split "," f; in
    builtins.replaceStrings [ "\"" ] [ "" ] (builtins.head parts);

  # Medidas numéricas de components.json, aplanadas: switch.small.width → switch-small-width
  flatten =
    prefix: set:
    builtins.concatLists (
      map (
        k:
        let
          v = set.${k};
          name = if prefix == "" then k else "${prefix}-${k}";
          ref = if builtins.isString v then match "(radius|space|size|duration)\\.(.+)" v else null;
        in
        if isAttrs v then
          flatten name v
        else if isInt v || isFloat v then
          [
            {
              inherit name;
              value = v;
              duration = false;
            }
          ]
        # "radius.sm", "duration.fast"… se resuelven contra tokens.json
        else if ref != null then
          [
            {
              inherit name;
              value = tokens.${builtins.head ref}.${builtins.elemAt ref 1};
              duration = builtins.head ref == "duration";
            }
          ]
        else
          [ ]
      ) (attrNames set)
    );
  metrics = flatten "" components.components;
  metricType =
    m:
    if m.duration then
      "duration"
    else if match ".*(weight|columns).*" m.name != null then
      "int"
    else if match ".*(opacity|scale).*" m.name != null then
      "float"
    else
      "length";
  metricValue =
    m:
    let
      t = metricType m;
    in
    if t == "length" then
      "${num m.value}px"
    else if t == "duration" then
      "${num m.value}ms"
    else
      num m.value;

  # Hojas de components.json para el CSS: nombre aplanado y valor crudo.
  componentLeaves =
    prefix: set:
    builtins.concatLists (
      map (
        k:
        let
          v = set.${k};
          name = if prefix == "" then k else "${prefix}-${k}";
        in
        if isAttrs v then
          componentLeaves name v
        else if isList v then
          [ ]
        else
          [ { inherit name; value = v; } ]
      ) (attrNames set)
    );
  leaves = componentLeaves "" components.components;

  unbrace = s: if match "[{].*[}]" s != null then substring 1 (stringLength s - 2) s else s;

  tokenRef =
    v:
    let
      m = match "(radius|space|size|duration)\\.(.+)" v;
    in
    if m == null then
      null
    else
      {
        group = elemAt m 0;
        key = elemAt m 1;
      };

  tokenVar =
    group: key:
    if group == "radius" then
      "radius-${key}"
    else if group == "space" then
      "space-${key}"
    else if group == "duration" then
      "dur-${key}"
    else
      key;

  isUnitless =
    name: match ".*(weight|columns|opacity|scale|mix|darken|lighten|exponent)" name != null;

  cssValue =
    name: v:
    if isInt v || isFloat v then
      let
        s = num v;
      in
      if match ".*-ms" name != null then
        "${s}ms"
      else if match ".*-angle" name != null then
        "${s}deg"
      else if match ".*wall-(light|dark)" name != null then
        "${builtins.replaceStrings [ ".000000" ] [ "" ] (num (v * 100.0))}%"
      else if isUnitless name then
        s
      else
        "${s}px"
    else if isString v then
      let
        ref = tokenRef v;
        bare = unbrace v;
        bareRef = tokenRef bare;
      in
      if ref != null then
        "var(--${tokenVar ref.group ref.key})"
      else if bareRef != null then
        "var(--${tokenVar bareRef.group bareRef.key})"
      else if builtins.elem bare colorNames then
        "var(--${bare})"
      else if match "#[0-9a-fA-F]{3,8}" v != null then
        v
      else if match ".*shadow" name != null then
        v
      else
        null
    else
      null;

  componentCss = lines (
    leaf: "  --${leaf.name}: ${cssValue leaf.name leaf.value};"
  ) (builtins.filter (leaf: cssValue leaf.name leaf.value != null) leaves);

  typeCss = lines (
    s:
    let
      st = tokens.font.styles.${s};
    in
    "  --type-${s}-size: ${num st.size}px; --type-${s}-line-height: ${num st.line-height}px; --type-${s}-weight: ${num st.weight}; --type-${s}-tracking: ${
      if st ? letter-spacing-em then "${num st.letter-spacing-em}em" else "0em"
    };"
  ) styleNames;

  styleNames = attrNames tokens.font.styles;
  sh = tokens.shadow.slint;

  slint = ''
    // Generado desde crates/vow-design/tengen (tokens.json, components.json). No editar a mano:
    // cambia los JSON y regenera con `nix eval --raw -f crates/vow-design/tengen slint`.
    export enum TengenMode { light, dark, dark-tint }

    export struct TengenAccent {
        id: string,
        color: color,
        label: string,
    }

    export global Theme {
        in-out property <TengenMode> mode: TengenMode.light;

        // Colores (por tema)
    ${lines (
      n:
      "    out property <color> ${n}: mode == TengenMode.dark ? ${tokens.themes.dark.${n}} : mode == TengenMode.dark-tint ? ${tokens.themes."dark-tint".${n}} : ${tokens.themes.light.${n}};"
    ) colorNames}

        // Redondeo, espacio, tamaños y duraciones
    ${lines (n: "    out property <length> radius-${n}: ${num tokens.radius.${n}}px;") (
      attrNames tokens.radius
    )}
    ${lines (n: "    out property <length> space-${n}: ${num tokens.space.${n}}px;") (
      attrNames tokens.space
    )}
    ${lines (n: "    out property <length> ${n}: ${num tokens.size.${n}}px;") (attrNames tokens.size)}
    ${lines (n: "    out property <duration> dur-${n}: ${num tokens.duration.${n}}ms;") (
      attrNames tokens.duration
    )}

        // Tipografía
        out property <string> font-sans: "${firstFamily tokens.font.families.sans}";
        out property <string> font-mono: "${firstFamily tokens.font.families.mono}";
        out property <string> font-script: "${firstFamily tokens.font.families.script}";
    ${lines (
      s: "    out property <length> text-${s}-size: ${num tokens.font.styles.${s}.size}px;"
    ) styleNames}
    ${lines (
      s: "    out property <int> text-${s}-weight: ${num tokens.font.styles.${s}.weight};"
    ) styleNames}

        // Sombras (Slint admite una por elemento)
        out property <length> shadow-window-offset-y: ${num sh.window."offset-y"}px;
        out property <length> shadow-window-blur: ${num sh.window.blur}px;
        out property <color> shadow-window-color: mode == TengenMode.light ? ${sh.window.color.light} : ${sh.window.color.dark};
        out property <length> shadow-float-offset-y: ${num sh.float."offset-y"}px;
        out property <length> shadow-float-blur: ${num sh.float.blur}px;
        out property <color> shadow-float-color: mode == TengenMode.light ? ${sh.float.color.light} : ${sh.float.color.dark};
        out property <length> shadow-knob-offset-y: ${num sh.knob."offset-y"}px;
        out property <length> shadow-knob-blur: ${num sh.knob.blur}px;
        out property <color> shadow-knob-color: ${sh.knob.color};

        // Acentos de sistema (GNOME), seleccionables junto al de Tengen.
        out property <[TengenAccent]> accents: [
    ${lines (
      a: "        { id: \"${a.id}\", color: ${a.color}, label: \"${a.label}\" },"
    ) tokens.accents}
        ];
    }

    // Medidas de components.json (switch-width, segmented-segment-radius, toolbar-tabs-tab-min-width…)
    export global Metrics {
    ${lines (m: "    out property <${metricType m}> ${m.name}: ${metricValue m};") metrics}
    }
  '';

  cssVars = th: lines (n: "  --${n}: ${tokens.themes.${th}.${n}};") colorNames;
  css = ''
    /* Generado desde crates/vow-design/tengen. data-theme="dark" o "dark-tint" cambia el subárbol. */
    :root, [data-theme="light"] {
    ${cssVars "light"}
      --shadow-window: ${tokens.shadow.window.light};
      --shadow-float: ${tokens.shadow.float.light};
    }
    ${lines
      (th: ''
        [data-theme="${th}"] {
        ${cssVars th}
          --shadow-window: ${tokens.shadow.window.${th}};
          --shadow-float: ${tokens.shadow.float.${th}};
        }'')
      [
        "dark"
        "dark-tint"
      ]
    }
    :root {
    ${lines (n: "  --radius-${n}: ${num tokens.radius.${n}}px;") (attrNames tokens.radius)}
    ${lines (n: "  --space-${n}: ${num tokens.space.${n}}px;") (attrNames tokens.space)}
    ${lines (n: "  --${n}: ${num tokens.size.${n}}px;") (attrNames tokens.size)}
    ${lines (n: "  --dur-${n}: ${num tokens.duration.${n}}ms;") (attrNames tokens.duration)}
      --shadow-knob: ${tokens.shadow.knob};
      --font-sans: ${tokens.font.families.sans};
      --font-mono: ${tokens.font.families.mono};
      --font-script: ${tokens.font.families.script};
    ${componentCss}
    ${typeCss}
    }
  '';

  qml = ''
    // Generado desde crates/vow-design/tengen. No editar a mano.
    pragma Singleton
    import QtQuick

    QtObject {
      readonly property string fontSans: "${firstFamily tokens.font.families.sans}"
      readonly property string fontMono: "${firstFamily tokens.font.families.mono}"
      readonly property string fontScript: "${firstFamily tokens.font.families.script}"
    ${lines (th: lines (n: "  readonly property color ${ident th}_${ident n}: ${tokens.themes.${th}.${n}};") colorNames) themes}
    ${lines (n: "  readonly property real radius_${ident n}: ${num tokens.radius.${n}};") (attrNames tokens.radius)}
    ${lines (n: "  readonly property real space_${ident n}: ${num tokens.space.${n}};") (attrNames tokens.space)}
    ${lines (n: "  readonly property real size_${ident n}: ${num tokens.size.${n}};") (attrNames tokens.size)}
    ${lines (n: "  readonly property real duration_${ident n}: ${num tokens.duration.${n}};") (attrNames tokens.duration)}
    ${lines (s: "  readonly property real text_${ident s}_size: ${num tokens.font.styles.${s}.size};") styleNames}
    ${lines (s: "  readonly property int text_${ident s}_weight: ${num tokens.font.styles.${s}.weight};") styleNames}
    ${lines (th: "  readonly property real shadow_window_${ident th}_blur: ${num tokens.shadow.layers.${th}.ambient.blur};\n  readonly property real shadow_window_${ident th}_offset_y: ${num tokens.shadow.layers.${th}.ambient.y-offset};\n  readonly property real shadow_window_${ident th}_opacity: ${num tokens.shadow.layers.${th}.ambient.opacity};") themes}
    ${lines (th: "  readonly property real shadow_contact_${ident th}_blur: ${num tokens.shadow.layers.${th}.contact.blur};\n  readonly property real shadow_contact_${ident th}_offset_y: ${num tokens.shadow.layers.${th}.contact.y-offset};\n  readonly property real shadow_contact_${ident th}_opacity: ${num tokens.shadow.layers.${th}.contact.opacity};") themes}
    }
  '';

in
rec {
  inherit
    tokens
    components
    themes
    slint
    css
    qml
    ;

  # Derivación con los archivos generados, para home-manager o un paquete:
  #   xdg.configFile."tengen".source = (import ./crates/vow-design/tengen).files pkgs;
  files =
    pkgs:
    pkgs.runCommand "tengen-design" { } ''
      mkdir -p $out
      cp ${pkgs.writeText "tengen.slint" slint} $out/tengen.slint
      cp ${pkgs.writeText "tengen.css" css} $out/tengen.css
      cp ${pkgs.writeText "Tengen.qml" qml} $out/Tengen.qml
      cp ${./tokens.json} $out/tokens.json
      cp ${./components.json} $out/components.json
    '';
}
