//! Emoji Plugin for Owlry
//!
//! A static provider that provides emoji search and copy functionality.
//! Requires wl-clipboard (wl-copy) for copying to clipboard.
//!
//! Examples:
//! - Search "smile" → 😀 😃 😄 etc.
//! - Search "heart" → ❤️ 💙 💚 etc.

use abi_stable::std_types::{ROption, RStr, RString, RVec};
use owlry_plugin_api::{
    owlry_plugin, PluginInfo, PluginItem, ProviderHandle, ProviderInfo, ProviderKind, API_VERSION,
};

// Plugin metadata
const PLUGIN_ID: &str = "emoji";
const PLUGIN_NAME: &str = "Emoji";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_DESCRIPTION: &str = "Search and copy emojis";

// Provider metadata
const PROVIDER_ID: &str = "emoji";
const PROVIDER_NAME: &str = "Emoji";
const PROVIDER_PREFIX: &str = ":emoji";
const PROVIDER_ICON: &str = "face-smile";
const PROVIDER_TYPE_ID: &str = "emoji";

/// Emoji provider state - holds cached items
struct EmojiState {
    items: Vec<PluginItem>,
}

impl EmojiState {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn load_emojis(&mut self) {
        self.items.clear();

        // Common emojis with searchable names
        // Format: (emoji, name, keywords)
        let emojis: &[(&str, &str, &str)] = &[
            // Smileys & Emotion
            ("😀", "grinning face", "smile happy"),
            ("😃", "grinning face with big eyes", "smile happy"),
            ("😄", "grinning face with smiling eyes", "smile happy laugh"),
            ("😁", "beaming face with smiling eyes", "smile happy grin"),
            ("😅", "grinning face with sweat", "smile nervous"),
            ("🤣", "rolling on the floor laughing", "lol rofl funny"),
            ("😂", "face with tears of joy", "laugh cry funny lol"),
            ("🙂", "slightly smiling face", "smile"),
            ("😊", "smiling face with smiling eyes", "blush happy"),
            ("😇", "smiling face with halo", "angel innocent"),
            ("🥰", "smiling face with hearts", "love adore"),
            ("😍", "smiling face with heart-eyes", "love crush"),
            ("🤩", "star-struck", "excited wow amazing"),
            ("😘", "face blowing a kiss", "kiss love"),
            ("😜", "winking face with tongue", "playful silly"),
            ("🤪", "zany face", "crazy silly wild"),
            ("😎", "smiling face with sunglasses", "cool"),
            ("🤓", "nerd face", "geek glasses"),
            ("🧐", "face with monocle", "thinking inspect"),
            ("😏", "smirking face", "smug"),
            ("😒", "unamused face", "meh annoyed"),
            ("🙄", "face with rolling eyes", "whatever annoyed"),
            ("😬", "grimacing face", "awkward nervous"),
            ("😮‍💨", "face exhaling", "sigh relief"),
            ("🤥", "lying face", "pinocchio lie"),
            ("😌", "relieved face", "relaxed peaceful"),
            ("😔", "pensive face", "sad thoughtful"),
            ("😪", "sleepy face", "tired"),
            ("🤤", "drooling face", "hungry yummy"),
            ("😴", "sleeping face", "zzz tired"),
            ("😷", "face with medical mask", "sick covid"),
            ("🤒", "face with thermometer", "sick fever"),
            ("🤕", "face with head-bandage", "hurt injured"),
            ("🤢", "nauseated face", "sick gross"),
            ("🤮", "face vomiting", "sick puke"),
            ("🤧", "sneezing face", "achoo sick"),
            ("🥵", "hot face", "sweating heat"),
            ("🥶", "cold face", "freezing"),
            ("😵", "face with crossed-out eyes", "dizzy dead"),
            ("🤯", "exploding head", "mind blown wow"),
            ("🤠", "cowboy hat face", "yeehaw western"),
            ("🥳", "partying face", "celebration party"),
            ("🥸", "disguised face", "incognito"),
            ("🤡", "clown face", "circus"),
            ("👻", "ghost", "halloween spooky"),
            ("💀", "skull", "dead death"),
            ("☠️", "skull and crossbones", "danger death"),
            ("👽", "alien", "ufo extraterrestrial"),
            ("🤖", "robot", "bot android"),
            ("💩", "pile of poo", "poop"),
            ("😈", "smiling face with horns", "devil evil"),
            ("👿", "angry face with horns", "devil evil"),
            // Gestures & People
            ("👋", "waving hand", "hello hi bye wave"),
            ("🤚", "raised back of hand", "stop"),
            ("🖐️", "hand with fingers splayed", "five high"),
            ("✋", "raised hand", "stop high five"),
            ("🖖", "vulcan salute", "spock trek"),
            ("👌", "ok hand", "okay perfect"),
            ("🤌", "pinched fingers", "italian"),
            ("🤏", "pinching hand", "small tiny"),
            ("✌️", "victory hand", "peace two"),
            ("🤞", "crossed fingers", "luck hope"),
            ("🤟", "love-you gesture", "ily rock"),
            ("🤘", "sign of the horns", "rock metal"),
            ("🤙", "call me hand", "shaka hang loose"),
            ("👈", "backhand index pointing left", "left point"),
            ("👉", "backhand index pointing right", "right point"),
            ("👆", "backhand index pointing up", "up point"),
            ("👇", "backhand index pointing down", "down point"),
            ("☝️", "index pointing up", "one point"),
            ("👍", "thumbs up", "like yes good approve"),
            ("👎", "thumbs down", "dislike no bad"),
            ("✊", "raised fist", "power solidarity"),
            ("👊", "oncoming fist", "punch bump"),
            ("🤛", "left-facing fist", "fist bump"),
            ("🤜", "right-facing fist", "fist bump"),
            ("👏", "clapping hands", "applause bravo"),
            ("🙌", "raising hands", "hooray celebrate"),
            ("👐", "open hands", "hug"),
            ("🤲", "palms up together", "prayer"),
            ("🤝", "handshake", "agreement deal"),
            ("🙏", "folded hands", "prayer please thanks"),
            ("✍️", "writing hand", "write"),
            ("💪", "flexed biceps", "strong muscle"),
            ("🦾", "mechanical arm", "robot prosthetic"),
            ("🦵", "leg", "kick"),
            ("🦶", "foot", "kick"),
            ("👂", "ear", "listen hear"),
            ("👃", "nose", "smell"),
            ("🧠", "brain", "smart think"),
            ("👀", "eyes", "look see watch"),
            ("👁️", "eye", "see look"),
            ("👅", "tongue", "taste lick"),
            ("👄", "mouth", "lips kiss"),
            // Hearts & Love
            ("❤️", "red heart", "love"),
            ("🧡", "orange heart", "love"),
            ("💛", "yellow heart", "love friendship"),
            ("💚", "green heart", "love"),
            ("💙", "blue heart", "love"),
            ("💜", "purple heart", "love"),
            ("🖤", "black heart", "love dark"),
            ("🤍", "white heart", "love pure"),
            ("🤎", "brown heart", "love"),
            ("💔", "broken heart", "heartbreak sad"),
            ("❤️‍🔥", "heart on fire", "passion love"),
            ("❤️‍🩹", "mending heart", "healing recovery"),
            ("💕", "two hearts", "love"),
            ("💞", "revolving hearts", "love"),
            ("💓", "beating heart", "love"),
            ("💗", "growing heart", "love"),
            ("💖", "sparkling heart", "love"),
            ("💘", "heart with arrow", "love cupid"),
            ("💝", "heart with ribbon", "love gift"),
            ("💟", "heart decoration", "love"),
            // Animals
            ("🐶", "dog face", "puppy"),
            ("🐱", "cat face", "kitty"),
            ("🐭", "mouse face", ""),
            ("🐹", "hamster", ""),
            ("🐰", "rabbit face", "bunny"),
            ("🦊", "fox", ""),
            ("🐻", "bear", ""),
            ("🐼", "panda", ""),
            ("🐨", "koala", ""),
            ("🐯", "tiger face", ""),
            ("🦁", "lion", ""),
            ("🐮", "cow face", ""),
            ("🐷", "pig face", ""),
            ("🐸", "frog", ""),
            ("🐵", "monkey face", ""),
            ("🦄", "unicorn", "magic"),
            ("🐝", "bee", "honeybee"),
            ("🦋", "butterfly", ""),
            ("🐌", "snail", "slow"),
            ("🐛", "bug", "caterpillar"),
            ("🦀", "crab", ""),
            ("🐙", "octopus", ""),
            ("🐠", "tropical fish", ""),
            ("🐟", "fish", ""),
            ("🐬", "dolphin", ""),
            ("🐳", "whale", ""),
            ("🦈", "shark", ""),
            ("🐊", "crocodile", "alligator"),
            ("🐢", "turtle", ""),
            ("🦎", "lizard", ""),
            ("🐍", "snake", ""),
            ("🦖", "t-rex", "dinosaur"),
            ("🦕", "sauropod", "dinosaur"),
            ("🐔", "chicken", ""),
            ("🐧", "penguin", ""),
            ("🦅", "eagle", "bird"),
            ("🦆", "duck", ""),
            ("🦉", "owl", ""),
            // Food & Drink
            ("🍎", "red apple", "fruit"),
            ("🍐", "pear", "fruit"),
            ("🍊", "orange", "tangerine fruit"),
            ("🍋", "lemon", "fruit"),
            ("🍌", "banana", "fruit"),
            ("🍉", "watermelon", "fruit"),
            ("🍇", "grapes", "fruit"),
            ("🍓", "strawberry", "fruit"),
            ("🍒", "cherries", "fruit"),
            ("🍑", "peach", "fruit"),
            ("🥭", "mango", "fruit"),
            ("🍍", "pineapple", "fruit"),
            ("🥥", "coconut", "fruit"),
            ("🥝", "kiwi", "fruit"),
            ("🍅", "tomato", "vegetable"),
            ("🥑", "avocado", ""),
            ("🥦", "broccoli", "vegetable"),
            ("🥬", "leafy green", "vegetable salad"),
            ("🥒", "cucumber", "vegetable"),
            ("🌶️", "hot pepper", "spicy chili"),
            ("🌽", "corn", ""),
            ("🥕", "carrot", "vegetable"),
            ("🧄", "garlic", ""),
            ("🧅", "onion", ""),
            ("🥔", "potato", ""),
            ("🍞", "bread", ""),
            ("🥐", "croissant", ""),
            ("🥖", "baguette", "bread french"),
            ("🥨", "pretzel", ""),
            ("🧀", "cheese", ""),
            ("🥚", "egg", ""),
            ("🍳", "cooking", "frying pan egg"),
            ("🥞", "pancakes", "breakfast"),
            ("🧇", "waffle", "breakfast"),
            ("🥓", "bacon", "breakfast"),
            ("🍔", "hamburger", "burger"),
            ("🍟", "french fries", ""),
            ("🍕", "pizza", ""),
            ("🌭", "hot dog", ""),
            ("🥪", "sandwich", ""),
            ("🌮", "taco", "mexican"),
            ("🌯", "burrito", "mexican"),
            ("🍜", "steaming bowl", "ramen noodles"),
            ("🍝", "spaghetti", "pasta"),
            ("🍣", "sushi", "japanese"),
            ("🍱", "bento box", "japanese"),
            ("🍩", "doughnut", "donut dessert"),
            ("🍪", "cookie", "dessert"),
            ("🎂", "birthday cake", "dessert"),
            ("🍰", "shortcake", "dessert"),
            ("🧁", "cupcake", "dessert"),
            ("🍫", "chocolate bar", "dessert"),
            ("🍬", "candy", "sweet"),
            ("🍭", "lollipop", "candy sweet"),
            ("🍦", "soft ice cream", "dessert"),
            ("🍨", "ice cream", "dessert"),
            ("☕", "hot beverage", "coffee tea"),
            ("🍵", "teacup", "tea"),
            ("🧃", "juice box", ""),
            ("🥤", "cup with straw", "soda drink"),
            ("🍺", "beer mug", "drink alcohol"),
            ("🍻", "clinking beer mugs", "cheers drink"),
            ("🥂", "clinking glasses", "champagne cheers"),
            ("🍷", "wine glass", "drink alcohol"),
            ("🥃", "tumbler glass", "whiskey drink"),
            ("🍸", "cocktail glass", "martini drink"),
            // Objects & Symbols
            ("💻", "laptop", "computer"),
            ("🖥️", "desktop computer", "pc"),
            ("⌨️", "keyboard", ""),
            ("🖱️", "computer mouse", ""),
            ("💾", "floppy disk", "save"),
            ("💿", "optical disk", "cd"),
            ("📱", "mobile phone", "smartphone"),
            ("☎️", "telephone", "phone"),
            ("📧", "email", "mail"),
            ("📨", "incoming envelope", "email"),
            ("📩", "envelope with arrow", "email send"),
            ("📝", "memo", "note write"),
            ("📄", "page facing up", "document"),
            ("📃", "page with curl", "document"),
            ("📑", "bookmark tabs", ""),
            ("📚", "books", "library read"),
            ("📖", "open book", "read"),
            ("🔗", "link", "chain url"),
            ("📎", "paperclip", "attachment"),
            ("🔒", "locked", "security"),
            ("🔓", "unlocked", "security open"),
            ("🔑", "key", "password"),
            ("🔧", "wrench", "tool fix"),
            ("🔨", "hammer", "tool"),
            ("⚙️", "gear", "settings"),
            ("🧲", "magnet", ""),
            ("💡", "light bulb", "idea"),
            ("🔦", "flashlight", ""),
            ("🔋", "battery", "power"),
            ("🔌", "electric plug", "power"),
            ("💰", "money bag", ""),
            ("💵", "dollar", "money cash"),
            ("💳", "credit card", "payment"),
            ("⏰", "alarm clock", "time"),
            ("⏱️", "stopwatch", "timer"),
            ("📅", "calendar", "date"),
            ("📆", "tear-off calendar", "date"),
            ("✅", "check mark", "done yes"),
            ("❌", "cross mark", "no wrong delete"),
            ("❓", "question mark", "help"),
            ("❗", "exclamation mark", "important warning"),
            ("⚠️", "warning", "caution alert"),
            ("🚫", "prohibited", "no ban forbidden"),
            ("⭕", "hollow circle", ""),
            ("🔴", "red circle", ""),
            ("🟠", "orange circle", ""),
            ("🟡", "yellow circle", ""),
            ("🟢", "green circle", ""),
            ("🔵", "blue circle", ""),
            ("🟣", "purple circle", ""),
            ("⚫", "black circle", ""),
            ("⚪", "white circle", ""),
            ("🟤", "brown circle", ""),
            ("⬛", "black square", ""),
            ("⬜", "white square", ""),
            ("🔶", "large orange diamond", ""),
            ("🔷", "large blue diamond", ""),
            ("⭐", "star", "favorite"),
            ("🌟", "glowing star", "sparkle"),
            ("✨", "sparkles", "magic shine"),
            ("💫", "dizzy", "star"),
            ("🔥", "fire", "hot lit"),
            ("💧", "droplet", "water"),
            ("🌊", "wave", "water ocean"),
            ("🎵", "musical note", "music"),
            ("🎶", "musical notes", "music"),
            ("🎤", "microphone", "sing karaoke"),
            ("🎧", "headphones", "music"),
            ("🎮", "video game", "gaming controller"),
            ("🕹️", "joystick", "gaming"),
            ("🎯", "direct hit", "target bullseye"),
            ("🏆", "trophy", "winner award"),
            ("🥇", "1st place medal", "gold winner"),
            ("🥈", "2nd place medal", "silver"),
            ("🥉", "3rd place medal", "bronze"),
            ("🎁", "wrapped gift", "present"),
            ("🎈", "balloon", "party"),
            ("🎉", "party popper", "celebration tada"),
            ("🎊", "confetti ball", "celebration"),
            // Arrows & Misc
            ("➡️", "right arrow", ""),
            ("⬅️", "left arrow", ""),
            ("⬆️", "up arrow", ""),
            ("⬇️", "down arrow", ""),
            ("↗️", "up-right arrow", ""),
            ("↘️", "down-right arrow", ""),
            ("↙️", "down-left arrow", ""),
            ("↖️", "up-left arrow", ""),
            ("↕️", "up-down arrow", ""),
            ("↔️", "left-right arrow", ""),
            ("🔄", "counterclockwise arrows", "refresh reload"),
            ("🔃", "clockwise arrows", "refresh reload"),
            ("➕", "plus", "add"),
            ("➖", "minus", "subtract"),
            ("➗", "division", "divide"),
            ("✖️", "multiply", "times"),
            ("♾️", "infinity", "forever"),
            ("💯", "hundred points", "100 perfect"),
            ("🆗", "ok button", "okay"),
            ("🆕", "new button", ""),
            ("🆓", "free button", ""),
            ("ℹ️", "information", "info"),
            ("🅿️", "parking", ""),
            ("🚀", "rocket", "launch startup"),
            ("✈️", "airplane", "travel flight"),
            ("🚗", "car", "automobile"),
            ("🚕", "taxi", "cab"),
            ("🚌", "bus", ""),
            ("🚂", "locomotive", "train"),
            ("🏠", "house", "home"),
            ("🏢", "office building", "work"),
            ("🏥", "hospital", ""),
            ("🏫", "school", ""),
            ("🏛️", "classical building", ""),
            ("⛪", "church", ""),
            ("🕌", "mosque", ""),
            ("🕍", "synagogue", ""),
            ("🗽", "statue of liberty", "usa america"),
            ("🗼", "tokyo tower", "japan"),
            ("🗾", "map of japan", ""),
            ("🌍", "globe europe-africa", "earth world"),
            ("🌎", "globe americas", "earth world"),
            ("🌏", "globe asia-australia", "earth world"),
            ("🌑", "new moon", ""),
            ("🌕", "full moon", ""),
            ("☀️", "sun", "sunny"),
            ("🌙", "crescent moon", "night"),
            ("☁️", "cloud", ""),
            ("🌧️", "cloud with rain", "rainy"),
            ("⛈️", "cloud with lightning", "storm thunder"),
            ("🌈", "rainbow", ""),
            ("❄️", "snowflake", "cold winter"),
            ("☃️", "snowman", "winter"),
            ("🎄", "christmas tree", "xmas holiday"),
            ("🎃", "jack-o-lantern", "halloween pumpkin"),
            ("🐚", "shell", "beach"),
            ("🌸", "cherry blossom", "flower spring"),
            ("🌺", "hibiscus", "flower"),
            ("🌻", "sunflower", "flower"),
            ("🌹", "rose", "flower love"),
            ("🌷", "tulip", "flower"),
            ("🌱", "seedling", "plant grow"),
            ("🌲", "evergreen tree", ""),
            ("🌳", "deciduous tree", ""),
            ("🌴", "palm tree", "tropical"),
            ("🌵", "cactus", "desert"),
            ("🍀", "four leaf clover", "luck irish"),
            ("🍁", "maple leaf", "fall autumn canada"),
            ("🍂", "fallen leaf", "fall autumn"),
        ];

        for (emoji, name, keywords) in emojis {
            self.items.push(
                PluginItem::new(
                    format!("emoji:{}", emoji),
                    name.to_string(),
                    format!("printf '%s' '{}' | wl-copy", emoji),
                )
                .with_description(format!("{} {}", emoji, keywords))
                .with_keywords(vec![name.to_string(), keywords.to_string()]),
            );
        }
    }
}

// ============================================================================
// Plugin Interface Implementation
// ============================================================================

extern "C" fn plugin_info() -> PluginInfo {
    PluginInfo {
        id: RString::from(PLUGIN_ID),
        name: RString::from(PLUGIN_NAME),
        version: RString::from(PLUGIN_VERSION),
        description: RString::from(PLUGIN_DESCRIPTION),
        api_version: API_VERSION,
    }
}

extern "C" fn plugin_providers() -> RVec<ProviderInfo> {
    vec![ProviderInfo {
        id: RString::from(PROVIDER_ID),
        name: RString::from(PROVIDER_NAME),
        prefix: ROption::RSome(RString::from(PROVIDER_PREFIX)),
        icon: RString::from(PROVIDER_ICON),
        provider_type: ProviderKind::Static,
        type_id: RString::from(PROVIDER_TYPE_ID),
    }]
    .into()
}

extern "C" fn provider_init(_provider_id: RStr<'_>) -> ProviderHandle {
    let state = Box::new(EmojiState::new());
    ProviderHandle::from_box(state)
}

extern "C" fn provider_refresh(handle: ProviderHandle) -> RVec<PluginItem> {
    if handle.ptr.is_null() {
        return RVec::new();
    }

    // SAFETY: We created this handle from Box<EmojiState>
    let state = unsafe { &mut *(handle.ptr as *mut EmojiState) };

    // Load emojis
    state.load_emojis();

    // Return items
    state.items.to_vec().into()
}

extern "C" fn provider_query(_handle: ProviderHandle, _query: RStr<'_>) -> RVec<PluginItem> {
    // Static provider - query is handled by the core using cached items
    RVec::new()
}

extern "C" fn provider_drop(handle: ProviderHandle) {
    if !handle.ptr.is_null() {
        // SAFETY: We created this handle from Box<EmojiState>
        unsafe {
            handle.drop_as::<EmojiState>();
        }
    }
}

// Register the plugin vtable
owlry_plugin! {
    info: plugin_info,
    providers: plugin_providers,
    init: provider_init,
    refresh: provider_refresh,
    query: provider_query,
    drop: provider_drop,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emoji_state_new() {
        let state = EmojiState::new();
        assert!(state.items.is_empty());
    }

    #[test]
    fn test_emoji_count() {
        let mut state = EmojiState::new();
        state.load_emojis();
        assert!(state.items.len() > 100, "Should have more than 100 emojis");
    }

    #[test]
    fn test_emoji_has_grinning_face() {
        let mut state = EmojiState::new();
        state.load_emojis();

        let grinning = state
            .items
            .iter()
            .find(|i| i.name.as_str() == "grinning face");
        assert!(grinning.is_some());

        let item = grinning.unwrap();
        assert!(item.description.as_ref().unwrap().as_str().contains("😀"));
    }

    #[test]
    fn test_emoji_command_format() {
        let mut state = EmojiState::new();
        state.load_emojis();

        let item = &state.items[0];
        assert!(item.command.as_str().contains("wl-copy"));
        assert!(item.command.as_str().contains("printf"));
    }

    #[test]
    fn test_emojis_have_keywords() {
        let mut state = EmojiState::new();
        state.load_emojis();

        // Check that items have keywords for searching
        let heart = state
            .items
            .iter()
            .find(|i| i.name.as_str() == "red heart");
        assert!(heart.is_some());
    }
}
