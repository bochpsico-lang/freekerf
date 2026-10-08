# FreeKerf: português (Brasil). Seeded from freekerf/design prototypes/app/i18n.js (e284367).


## mode
mode-design = Projetar
mode-produce = Produzir
mode-narrowNote = A edição fica disponível em telas a partir de 820 px. Aqui você controla e acompanha a máquina.

## estop
estop = PARAR
estop-hint = Parar agora: desliga o laser e reinicia o controlador (Shift+Esc). Não substitui o botão de emergência físico da máquina.

## state
state-offline = Desconectada
state-connecting = Conectando…
state-idle = Pronta
state-run = Executando
state-hold = Em pausa
state-alarm = Alarme
state-home = Referenciando
state-jog = Movendo
state-frame = Contornando

## file
file-unsaved = alterações não salvas

## tool
tool-select = Selecionar (V)
tool-node = Editar nós (N)
tool-rect = Retângulo (R)
tool-ellipse = Elipse (E)
tool-text = Texto (T)
tool-import = Importar SVG, DXF ou imagem (Ctrl+I)
tool-trace = Vetorizar imagem
tool-weld = Soldar
tool-union = União
tool-subtract = Subtrair
tool-intersect = Interseção
tool-offset = Deslocar contorno (offset)
tool-array = Matriz (array)
tool-test = Teste de material
tool-undo = Desfazer (Ctrl+Z)
tool-redo = Refazer (Ctrl+Shift+Z)
tool-camera = Câmera e sobreposição

## view
view-zoomIn = Aproximar (+)
view-zoomOut = Afastar (−)
view-fit = Ajustar à mesa (F)
view-multi = Seleção múltipla
view-marquee = Seleção por área

## panel
panel-layers = Camadas
panel-object = Objeto
panel-machine = Máquina
panel-console = Console
panel-close = Fechar painel

## layers
layers-output = Enviar ao laser
layers-show = Mostrar no canvas
layers-mode = Modo
layers-mode-line = Linha
layers-mode-fill = Preencher
layers-mode-image = Imagem
layers-mode-tool = Ferramenta
layers-speed = Velocidade
layers-power = Potência
layers-passes = Passadas
layers-interval = Intervalo de linhas
layers-air = Assistência de ar
layers-material = Aplicar material da biblioteca…
layers-up = Subir na ordem de saída
layers-down = Descer na ordem de saída
layers-empty = Nenhuma camada ainda. Desenhe algo ou escolha uma cor na paleta.
layers-objects = { $n } obj.
layers-edit = Editar parâmetros da camada
layers-order = Ordem de saída: de cima para baixo
layers-toolNote = Camada de ferramenta: aparece no canvas e serve de guia, mas nunca vai para o laser.

## palette
palette-hint = Cor = camada. Com objetos selecionados, toque numa cor para mudar a camada.

## obj
obj-none = Nada selecionado. Toque num objeto ou desenhe um novo.
obj-count = { $n } objetos selecionados
obj-x = X
obj-y = Y
obj-w = Largura
obj-h = Altura
obj-rot = Rotação
obj-lock = Manter proporção
obj-layer = Camada
obj-text = Texto
obj-size = Tamanho
obj-duplicate = Duplicar (Ctrl+D)
obj-delete = Excluir (Del)
obj-front = Trazer para frente
obj-back = Enviar para trás
obj-align = Alinhar
obj-alignLeft = Alinhar à esquerda
obj-alignCenterH = Centralizar na horizontal
obj-alignRight = Alinhar à direita
obj-alignTop = Alinhar ao topo
obj-alignMiddleV = Centralizar na vertical
obj-alignBottom = Alinhar à base
obj-type-rect = Retângulo
obj-type-ellipse = Elipse
obj-type-text = Texto
obj-type-path = Caminho
obj-type-image = Imagem
obj-image = Ajustes de imagem
obj-brightness = Brilho
obj-contrast = Contraste
obj-dither = Pontilhado

## machine
machine-emulator = Emulador FreeKerf (sem máquina)
machine-refreshPorts = Procurar portas de novo
machine-openGcode = Abrir G-code…
machine-noProgram = Nenhum programa carregado
machine-lines = Linhas
machine-overrides = Ajustes em tempo real
machine-ovFeed = Velocidade
machine-ovPower = Potência
machine-ovRapid = Movimentos rápidos
machine-noButtons = Nenhum botão personalizado ainda.
machine-laserLife = Uso do módulo laser
machine-connection = Conexão
machine-port = Porta
machine-connect = Conectar
machine-disconnect = Desconectar
machine-detected = { $fw } detectado · área { $w }×{ $h } mm
machine-position = Posição
machine-jog = Mover (jog)
machine-step = Passo
machine-continuous = Contínuo (segure para mover)
machine-jogSpeed = Velocidade
machine-home = Referenciar
machine-unlock = Desbloquear
machine-setOrigin = Origem aqui
machine-goOrigin = Ir à origem
machine-frame = Contorno
machine-frameHint = Percorre o retângulo do trabalho com o laser desligado.
machine-jobOrigin = Início do trabalho
machine-jobOrigin-abs = Coordenadas absolutas
machine-jobOrigin-current = Posição atual
machine-jobOrigin-user = Origem do usuário
machine-anchor = Ponto de ancoragem
machine-start = Iniciar
machine-pause = Pausar
machine-resume = Continuar
machine-stop = Parar trabalho
machine-job = Trabalho
machine-elapsed = Decorrido
machine-remaining = Restante
machine-estimate = Estimativa
machine-layerNow = Camada atual
machine-line = Linha { $a } de { $b }
machine-custom = Botões personalizados
machine-addButton = Novo botão
machine-simSpeed = Velocidade da simulação
machine-simulate = Simular falha
machine-simAlarm = Alarme de limite
machine-simDisconnect = Queda de conexão
machine-notConnected = Conecte a máquina para mover, contornar ou gravar.
machine-noJob = Nada para gravar: nenhuma camada está marcada para enviar ao laser.

## console
console-placeholder = Digite G-code ou um comando ($$, $H, ?)…
console-send = Enviar
console-hideStatus = Ocultar relatórios de status

## preflight
preflight-title = Pronto para gravar?
preflight-summary = { $n } camadas · { $t } · { $w }×{ $h } mm
preflight-c1 = Óculos de proteção certos para este laser
preflight-c2 = Exaustão e assistência de ar ligadas
preflight-c3 = Material preso e foco ajustado
preflight-c4 = Extintor por perto; não deixe a máquina sozinha
preflight-fire = Disparar laser
preflight-wait = Disparar em { $s } s
preflight-cancel = Cancelar
preflight-note = Você pode pausar a qualquer momento com Esc ou parar tudo com PARAR.

## alarm
alarm-title = Alarme { $n }
alarm-body = O controlador parou o movimento e desligou o laser na linha { $n }. Confira se nada bloqueia o cabeçote antes de continuar.

## disc
disc-title = Conexão perdida
disc-body = A máquina parou de responder na linha { $n }. O controlador desligou o laser.

## recover
recover-restoreWco = Restaurar a origem de trabalho do trabalho
recover-nothing = Nenhum trabalho interrompido.
recover-open = Recuperar trabalho…
recover-title = Recuperar trabalho
recover-cause = Causa
recover-fromStart = Desde o início
recover-fromBefore = Algumas linhas antes da interrupção (linha { $n })
recover-recommended = recomendado
recover-fromSent = A partir da última linha enviada ({ $n })
recover-fromLine = A partir da linha
recover-homing = Referenciar ($H) antes de continuar
recover-wco = Restaurar origem de trabalho X{ $x } Y{ $y }
recover-go = Continuar daqui
recover-dismiss = Agora não

## toast
toast-layerChanged = Movido para a camada { $c }.
toast-jobDone = Trabalho concluído em { $t }.
toast-stopped = Trabalho parado. Laser desligado.
toast-estop = PARADA: laser desligado e controlador reiniciado. Desbloqueie para continuar.
toast-connected = Conectada: { $fw }
toast-frameDone = Contorno concluído.
toast-material = Material “{ $m }” aplicado à camada { $c }.
toast-unlocked = Desbloqueada. Confira a posição antes de mover.
toast-homed = Referenciamento concluído.
toast-origin = Origem de trabalho definida aqui.

## settings
settings-tab-display = Exibição
settings-tab-machine = Máquina
settings-tab-job = Código do trabalho
settings-tab-safety = Segurança
settings-tab-automation = Automação
settings-langAuto = Automático (idioma do sistema)
settings-firmware = Firmware
settings-baud = Velocidade (baud)
settings-streaming = Envio
settings-streaming-Buffered = Com buffer (rápido)
settings-streaming-Synchronous = Síncrono (seguro)
settings-streaming-RepeatOnError = Repetir em caso de erro
settings-statusMs = Estado a cada (ms)
settings-resetOnConnect = Reiniciar a placa ao conectar
settings-supportPwm = PWM por hardware
settings-autoCooling = Pausas automáticas de resfriamento
settings-jogSpeed = Velocidade do jog (mm/min)
settings-header = Cabeçalho
settings-passes = Entre passadas
settings-footer = Rodapé
settings-jobHint = G-code enviado antes do trabalho, entre passadas e depois dele. [expr] é avaliado; !, ~, ?, ctrl-x e 0xNN são enviados na hora.
settings-countdown = Contagem antes de "Disparar laser"
settings-userOnly = Só você pode mudar isso, com `freekerf config set` (scripts e agentes não podem).
settings-mcp = Agentes de IA (servidor MCP)
settings-mcpMachine = Agentes podem mover a máquina
settings-mcpLaser = Agentes podem pedir para disparar o laser
settings-on = Ligado
settings-off = Desligado
settings-automationNote = Desligado por padrão. Mesmo quando permitido, o laser só dispara depois que você confirma nesta tela.
settings-title = Exibição e entrada
settings-theme = Tema

## theme
theme-dark = Escuro
theme-light = Claro

## settings
settings-contrast = Alto contraste
settings-density = Densidade

## density
density-auto = Automática
density-compact = Compacta (mouse)
density-comfortable = Confortável (toque)

## settings
settings-lang = Idioma
settings-units = Unidades
settings-welcome = Mostrar boas-vindas de novo
settings-shortcuts = Atalhos de teclado

## status
status-zoom = Zoom
status-head = Cabeçote
status-sel = Seleção

## units
units-mm = mm
units-in = pol

## welcome
welcome-title = Bem-vindo ao FreeKerf
welcome-sub = Design e controle livres para a sua máquina laser.
welcome-setup = Configurar minha máquina
welcome-migrate = Importar do LaserGRBL
welcome-explore = Explorar com o simulador
welcome-origin = Evolução do LaserGRBL, criado por Diego Settimi. Software livre sob GPL-3.0-or-later.
welcome-kindTitle = Que máquina você tem?
welcome-kind-diode = Laser de diodo
welcome-kind-diodeSub = GRBL ou grblHAL, 5–40 W ópticos
welcome-kind-co2 = Laser CO₂
welcome-kind-co2Sub = GRBL, grblHAL, Smoothie ou Marlin
welcome-kind-other = Outra ou genérica
welcome-kind-otherSub = Plotter, fresadora leve, firmware próprio
welcome-area = Área de trabalho
welcome-next = Avançar
welcome-back = Voltar
welcome-finish = Começar
welcome-connTitle = Vamos encontrar a máquina
welcome-found = Encontramos:
welcome-searching = Procurando portas USB e rede…
welcome-migrateTitle = Importar do LaserGRBL
welcome-migrateNote = Encontramos uma instalação do LaserGRBL. Os arquivos originais não são alterados.
welcome-migrateGo = Importar selecionados
welcome-ready = Tudo pronto
welcome-readySub = Deixamos um projeto de exemplo aberto. Experimente arrastar, girar e trocar cores.

## shortcuts
shortcuts-title = Atalhos de teclado

## common
common-skip-canvas = Pular para o canvas
common-soon = Chega numa próxima atualização.
common-close = Fechar
common-ok = OK
common-menu = Menu

## ctx
ctx-title = Ações

## jog
jog-n = Mover para trás (Y+)
jog-s = Mover para a frente (Y−)
jog-w = Mover para a esquerda (X−)
jog-e = Mover para a direita (X+)

## drawer
drawer-toggle = Painéis

## test
test-title = Teste de material
test-desc = Grade de potência × velocidade para encontrar os parâmetros do material.

## toast
toast-needConn = Conecte a máquina primeiro.
toast-needSel = Selecione um ou mais objetos primeiro.

## estop
estop-title = Parada acionada
estop-body = Laser desligado e controlador reiniciado na linha { $n }. A posição pode ter se perdido: referencie antes de continuar.

## cause
cause-other = interrompido
cause-alarm = alarme do controlador
cause-disc = conexão perdida
cause-estop = parada manual (PARAR)

## obj
obj-dither-fs = Floyd–Steinberg
obj-dither-atk = Atkinson
obj-dither-thr = Limiar
obj-center = Posição pelo centro do objeto

## welcome
welcome-skip = Pular

## sb
sb-auto = auto

## kb
kb-g-tools = Ferramentas
kb-g-edit = Edição
kb-g-view = Vista
kb-g-machine = Máquina
kb-undo = Desfazer
kb-redo = Refazer
kb-dup = Duplicar
kb-del = Excluir
kb-all = Selecionar tudo
kb-nudge = Mover a seleção 1 mm (Shift: 10 mm, Alt: 0,1 mm)
kb-constrain = Ao arrastar: Shift trava eixo/proporção; Alt redimensiona a partir do centro
kb-esc = Cancelar ferramenta ou limpar seleção
kb-zoom = Aproximar / afastar
kb-fit = Ajustar à mesa
kb-pan = Mover a vista (segure e arraste)
kb-wheel = Zoom no cursor (Shift: rolar na horizontal)
kb-mode = Projetar / Produzir
kb-start = Iniciar (abre a confirmação)
kb-pause = Pausar / continuar
kb-frame = Contorno
kb-hold = Pausar o trabalho em execução
kb-estop = PARAR (funciona em qualquer lugar)
