//! The interface's words in English and Russian.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Russian,
}

impl Language {
    pub fn other(self) -> Language {
        match self {
            Language::English => Language::Russian,
            Language::Russian => Language::English,
        }
    }
}

macro_rules! strings {
    ($($id:ident: $english:literal, $russian:literal;)*) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        pub enum Str {
            $($id),*
        }

        const TABLE: &[(&str, &str)] = &[$(($english, $russian)),*];
    };
}

strings! {
    Title: "3D Go", "Го 3D";
    BlackToMove: "Black to play", "Ход чёрных";
    WhiteToMove: "White to play", "Ход белых";
    Thinking: "Computer is thinking…", "Компьютер думает…";
    Counting: "Counting", "Подсчёт очков";
    CountingHelp: "Click a group to mark it dead or alive", "Щелчок по группе: мёртвая / живая";
    Estimating: "Finding the dead stones…", "Ищу мёртвые камни…";
    Draw: "Jigo: a draw", "Ничья (дзиго)";
    BlackWins: "Black wins", "Чёрные победили";
    WhiteWins: "White wins", "Белые победили";
    BlackResigned: "Black resigned", "Чёрные сдались";
    WhiteResigned: "White resigned", "Белые сдались";
    ScoreDetail: "Black {} · White {} + {} komi", "Чёрные {} · белые {} + {} коми";
    BlackPasses: "Black passes", "Чёрные пасуют";
    WhitePasses: "White passes", "Белые пасуют";
    BothPassed: "Both passed: time to count", "Оба спасовали — подсчёт очков";
    Pass: "Pass", "Пас";
    Resign: "Resign", "Сдаться";
    ConfirmResign: "Sure?", "Точно?";
    Accept: "Accept", "Принять";
    Resume: "Play on", "Доиграть";
    Estimate: "Estimate", "Оценка";
    EstimateToast: "{} ahead by about {}", "{} впереди примерно на {}";
    You: "You", "Вы";
    Computer: "Computer", "Компьютер";
    Player: "Player", "Игрок";
    Black: "Black", "Чёрные";
    White: "White", "Белые";
    Captures: "prisoners", "пленные";
    Points: "points", "очки";
    Komi: "komi", "коми";
    Moves: "Moves", "Ходы";
    NoMovesYet: "No moves yet", "Ходов пока нет";
    Undo: "Undo", "Назад";
    Redo: "Redo", "Вперёд";
    Hint: "Hint", "Подсказка";
    NewGame: "New game", "Новая партия";
    Flip: "Flip board", "Развернуть";
    Mode: "Mode", "Режим";
    ModeBlack: "You play Black", "Вы за чёрных";
    ModeWhite: "You play White", "Вы за белых";
    ModeTwoPlayers: "Two players", "Два игрока";
    ModeDemo: "Demo game", "Демонстрация";
    Level: "Level", "Уровень";
    Level1: "Novice", "Новичок";
    Level2: "Beginner", "Начинающий";
    Level3: "Casual", "Любитель";
    Level4: "Strong", "Сильный";
    Level5: "Master", "Мастер";
    Board: "Board", "Доска";
    Handicap: "Handicap", "Фора";
    NoHandicap: "None", "Нет";
    StonesFew: "{} stones", "{} камня";
    StonesMany: "{} stones", "{} камней";
    NextGame: "Applies from the next game (N)", "Вступит в силу в новой партии (N)";
    Coordinates: "Coordinates", "Координаты";
    Theme: "Theme", "Тема";
    Sound: "Sound", "Звук";
    LanguageName: "English", "Русский";
    Footer: "F1 help · Tab panel · F11 full screen", "F1 справка · Tab панель · F11 весь экран";
    Controls: "Controls", "Управление";
    KeyClick: "Click", "Щелчок";
    KeyDrag: "Drag", "Перетаскивание";
    KeyWheel: "Wheel", "Колесо мыши";
    HelpClick: "Place a stone; mark dead groups", "Поставить камень; отметить мёртвых";
    HelpRotate: "Turn the view (either button)", "Повернуть вид (любой кнопкой)";
    HelpZoom: "Zoom in and out", "Приблизить или отдалить";
    HelpUndo: "Take back", "Отменить ход";
    HelpRedo: "Replay", "Вернуть ход";
    HelpPass: "Pass", "Пас";
    HelpHint: "Suggest a move", "Подсказать ход";
    HelpEstimate: "Estimate the score", "Оценить положение";
    HelpAccept: "Accept the count", "Принять подсчёт";
    HelpNew: "New game", "Новая партия";
    HelpFlip: "Flip the board", "Развернуть доску";
    HelpTopView: "View from above", "Вид сверху";
    HelpLevel: "Computer level", "Уровень компьютера";
    HelpMode: "Who plays whom", "Кто с кем играет";
    HelpBoard: "Board size", "Размер доски";
    HelpCoordinates: "Coordinates", "Координаты";
    HelpTheme: "Next theme", "Следующая тема";
    HelpSound: "Sound on or off", "Звук вкл/выкл";
    HelpLanguage: "English / Russian", "Русский / английский";
    HelpPanel: "Show or hide the panel", "Показать/скрыть панель";
    HelpFullScreen: "Full screen", "Весь экран";
    HelpCopy: "Copy the game (SGF)", "Копировать партию (SGF)";
    HelpPaste: "Paste a game (SGF)", "Вставить партию (SGF)";
    HelpClose: "Close this help", "Закрыть справку";
    Rules: "Rules", "Правила";
    RulesNote: "Black and White take turns placing stones where the lines cross. Stones of one color next to each other along the lines form a group; a group with no empty point beside it is captured. A ko may not be retaken at once, and no move may bring back an earlier position. Two passes in a row end the game: the dead groups are taken off, and each side counts its stones and the empty points they surround. White gets komi: 7.5 points, or 0.5 with a handicap.",
        "Чёрные и белые по очереди ставят камни на пересечения линий. Камни одного цвета, соседние по линиям, образуют группу; группа без свободных соседних пунктов снимается с доски. Ко нельзя отбить сразу же, и никакой ход не может повторить прежнюю позицию. Два паса подряд завершают партию: мёртвые группы снимаются, и каждый считает свои камни и окружённые ими пустые пункты. Белые получают коми: 7,5 очка, а при форе 0,5.";
    GameCopied: "Game copied as SGF", "Партия скопирована (SGF)";
    GameLoaded: "Game loaded", "Партия загружена";
    NothingToPaste: "The clipboard has no SGF game", "В буфере нет партии в SGF";
    HintToast: "Hint: ", "Подсказка: ";
    HintPass: "Hint: pass", "Подсказка: пас";
    NothingToUndo: "No moves to take back", "Нечего отменять";
    Occupied: "That point is taken", "Пункт занят";
    KoForbidden: "Ko: it may not be retaken at once", "Ко: сразу отбивать нельзя";
    Suicide: "Suicide is not allowed", "Самоубийственный ход запрещён";
    Superko: "Not allowed: the position would repeat", "Нельзя: позиция повторится";
}

impl Str {
    /// The name of a computer level from 1 to 5.
    pub fn level(level: i32) -> Str {
        [Str::Level1, Str::Level2, Str::Level3, Str::Level4, Str::Level5][(level.clamp(1, 5) - 1) as usize]
    }
}

pub fn tr(id: Str, language: Language) -> &'static str {
    let (english, russian) = TABLE[id as usize];
    match language {
        Language::English => english,
        Language::Russian => russian,
    }
}

/// A text with each "{}" in it replaced by the next of the values.
pub fn fill(text: &str, values: &[&dyn std::fmt::Display]) -> String {
    let mut parts = text.split("{}");
    let mut filled = parts.next().unwrap_or_default().to_owned();
    for (part, value) in parts.zip(values) {
        filled += &value.to_string();
        filled += part;
    }
    filled
}
