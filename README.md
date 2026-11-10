# RoboBackup

Programa de backup para Windows feito para quem nunca usou o terminal. Ele usa o **Robocopy** (que já vem no Windows) por trás de uma interface simples em português.

## O que ele faz

- **Rotinas de backup**: você escolhe o que copiar, para onde, como e quando, num assistente de 5 passos.
- **Destinos**: HD externo/pendrive (acha o disco mesmo se a letra mudar), pasta na rede (NAS/servidor, com usuário e senha opcionais) ou outro disco.
- **3 modos**:
  - *Cópia simples* (recomendado): nunca apaga nada do backup.
  - *Versões com data*: uma pasta por execução, guardando as últimas N.
  - *Espelho*: o destino fica idêntico à origem e apaga o que sumiu. Pede confirmação antes de ativar.
- **Agendamento pelo Agendador de Tarefas do Windows**: roda como SYSTEM, com o programa fechado e sem ninguém logado. Se o PC estava desligado no horário, roda assim que ligar.
- **Acompanhamento**: barra de progresso, velocidade, tempo restante e botão **Parar**. Mostra também os backups iniciados pelo agendador.
- **Histórico** com resumo em português, lista de arquivos com problema e o registro técnico completo.
- **Restaurar**: tudo, uma pasta ou um arquivo, para uma pasta separada (padrão) ou para o lugar original.
- **Avisos** do Windows quando um backup termina ou falha. Na tela inicial, um "semáforo" mostra se os backups estão em dia, atrasados ou com falha.
- **PIN de administrador** (opcional): impede que a equipe altere ou exclua rotinas por engano.
- **Inicia com o Windows**, minimizado perto do relógio.

## Instalação na pousada

1. Rode `RoboBackup_1.0.0_x64-setup.exe`. Ele instala para todos os usuários e pede administrador.
2. Abra o RoboBackup e clique em **Preparar o computador**. Aceite o aviso do Windows (só uma vez).
3. Clique em **Nova rotina de backup** e siga o assistente. Ao salvar, o Windows pede permissão de novo para criar o agendamento.
4. (Recomendado) Em **Configurações**, ative um **PIN**.
5. Clique em **Executar agora** uma vez para fazer o primeiro backup e confira se aparece em verde.

Os dados ficam em `C:\ProgramData\RoboBackup` (banco `robobackup.db`, `logs\`). As senhas de rede ficam em `secrets\`, cifradas com DPAPI da máquina e acessíveis só para administradores e SYSTEM.

## Para desenvolvedores

Requisitos: Node LTS, Rust (MSVC) e Visual Studio Build Tools com C++.

```bash
npm install
npm run tauri dev
```

```bash
npm run tauri build
```

O instalador sai em `src-tauri/target/release/bundle/nsis/`.

```bash
cd src-tauri && cargo test
```

### Modos do executável

| Comando | Uso |
|---|---|
| `robobackup.exe` | Interface (com `--minimized` ao iniciar com o Windows) |
| `robobackup.exe --run <id>` | Executa a rotina sem janela (usado pelo Agendador) |
| `robobackup.exe --restore <arquivo.json>` | Restauração sem janela (iniciada já elevada) |
| `robobackup.exe --admin-op <arquivo.json>` | Operações de admin em lote (setup, agendar, salvar senha) |

### Estrutura

- `src-tauri/src/robocopy.rs`: argumentos, leitura da saída (inclusive em pt-BR) e códigos de saída
- `src-tauri/src/runner.rs`: executa backup e restauração, versões e limpeza
- `src-tauri/src/scheduler.rs`: XML do Agendador de Tarefas
- `src-tauri/src/win.rs`: rede (WNet), DPAPI, volumes e UAC
- `src-tauri/src/elevation.rs`: operações administrativas
- `src/pages/*`: telas (Início, Rotinas, Assistente, Histórico, Restaurar, Configurações)

Cada rotina vira a tarefa `\RoboBackup\<id>` no Agendador. Se os agendamentos sumirem ou o programa mudar de pasta, use **Configurações → Reparar agendamentos**.
