//! FastTrading for Kestrel: a Speed Trade button on the merchant
//! screen that repeats the selected trade, as fast as the speed
//! setting allows. A behaviour port of FastTrading by bendy1234
//! (MIT), completing the trade loop on the API 22 screen surface
//! and the API 24 inventory-actions surface, as the API 24 design
//! prescribes: `screen.select-trade` picks the trade (the game
//! stages the payments and the result, as a click on the trade
//! list would), the staged result is watched for in the container
//! read, and the take is ONE click on the merchant result slot
//! (internal slot 650) — a quick-move, which keeps the hand empty
//! for the next round. The loop stops when the offer locks or is
//! used up, when the payments run out, when the result has nowhere
//! to go, when the merchant screen closes, or when the button is
//! pressed again.
//!
//! Fidelity, against the registry entry: the speed-trade button,
//! the repeat, the speed setting and the command that opens the
//! controls all match. The registry names no numbers for the
//! speed, so the setting steps over idle ticks between rounds
//! (Fastest is none — a round every two ticks) and the default is
//! Fastest, as "repeats the selected trade quickly" asks. One gap,
//! a surface limit: the loop runs on ticks, not frames, so a round
//! takes two ticks at the fastest (pick on one tick, take on the
//! next) where the original could repeat within a frame.

use horizon_mod_sdk::{
    ClickKind, Mod, Registration, ScreenButton, ScreenEvent, TradeList, TradeStack,
    inventory, inventory_actions, screen, storage,
    ui::{self, ButtonWidget, TextWidget, Widget},
};

#[cfg(test)]
const API_LEVEL: u32 = 24;

/// The merchant's result slot, in the inventory numbering.
const RESULT_SLOT: u32 = 650;
/// How many ticks the take phase waits for a staged result to
/// appear before giving the round up.
const TAKE_WAIT_LIMIT: u32 = 5;

// The screen button (API 22) and the widgets of the settings page
// and the /fasttrading controls screen.
const BTN_SPEED_TRADE: u32 = 1;
const SET_SPEED: u32 = 101;
const CTL_TOGGLE: u32 = 201;
const CTL_SPEED: u32 = 202;

const SPEED_KEY: &str = "speed";

/// Idle ticks between rounds, by settings index. A round itself
/// takes two ticks (pick, take); Fastest adds none.
const SPEEDS: [u32; 5] = [0, 2, 5, 10, 20];
const SPEED_LABELS: [&str; 5] = ["Fastest", "Fast", "Medium", "Slow", "Very slow"];

/// Where the loop stands.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    /// Pick the selected trade.
    Select,
    /// Watch for the staged result and take it.
    Take,
    /// Idle ticks between rounds, at the speed setting.
    Cooldown,
}

/// What the loop does this tick — the pure decision, unit-tested
/// below.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Decision {
    Select(u32),
    Take,
    Wait,
    Stop(&'static str),
}

/// Decides the loop's next move from the trade list, the payment
/// check and the container read. `has` answers whether the player
/// holds a cost stack; `result_present` whether the merchant's
/// result slot holds the staged result.
fn decide(
    phase: Phase,
    trades: &TradeList,
    has: &dyn Fn(&TradeStack) -> bool,
    result_present: bool,
    take_wait: u32,
) -> Decision {
    match phase {
        Phase::Select => {
            let Some(selected) = trades.selected else {
                return Decision::Stop("no trade selected");
            };
            let Some(offer) = trades.offers.get(selected as usize) else {
                return Decision::Stop("no trade selected");
            };
            if !offer.available {
                return Decision::Stop("the trade is locked");
            }
            if offer.max_uses > 0 && offer.uses >= offer.max_uses {
                return Decision::Stop("the trade is used up");
            }
            if result_present {
                return Decision::Stop("no room for the result");
            }
            if !has(&offer.cost_a) || offer.cost_b.as_ref().is_some_and(|b| !has(b)) {
                return Decision::Stop("out of payment");
            }
            Decision::Select(selected)
        }
        Phase::Take => {
            if result_present {
                Decision::Take
            } else if take_wait >= TAKE_WAIT_LIMIT {
                Decision::Stop("the trade did not stage")
            } else {
                Decision::Wait
            }
        }
        Phase::Cooldown => Decision::Wait,
    }
}

/// Whether the player holds at least `cost.count` of the cost's
/// item, exact components included, across the inventory's slots.
fn holds(slots: &[inventory::InvStack], cost: &TradeStack) -> bool {
    slots
        .iter()
        .filter(|s| s.id == cost.item && s.components == cost.components)
        .map(|s| s.count)
        .sum::<u32>()
        >= cost.count
}

struct FastTrading {
    speed: usize,
    on: bool,
    phase: Phase,
    cooldown: u32,
    take_wait: u32,
    note: String,
    shown_note: String,
    controls_open: bool,
    declared: String,
}

impl FastTrading {
    fn widgets(&self) -> Vec<Widget> {
        vec![
            Widget::Text(TextWidget {
                id: 0,
                text: format!("FastTrading: {}", self.note),
                color: 0,
            }),
            Widget::Button(ButtonWidget {
                id: CTL_TOGGLE,
                label: format!("Speed trade: {}", if self.on { "on" } else { "off" }),
            }),
            Widget::Button(ButtonWidget {
                id: CTL_SPEED,
                label: format!("Trade speed: {}", SPEED_LABELS[self.speed]),
            }),
        ]
    }

    fn refresh_settings(&self) {
        ui::set_settings(&[Widget::Button(ButtonWidget {
            id: SET_SPEED,
            label: format!("Trade speed: {}", SPEED_LABELS[self.speed]),
        })]);
    }

    fn open_controls(&mut self) {
        self.controls_open = true;
        self.shown_note = self.note.clone();
        ui::open_screen(&ui::Screen {
            title: "FastTrading".to_string(),
            widgets: self.widgets(),
        });
    }

    fn cycle_speed(&mut self) {
        self.speed = (self.speed + 1) % SPEEDS.len();
        let _ = storage::set(SPEED_KEY, self.speed.to_string().as_bytes());
    }

    fn toggle(&mut self) {
        self.on = !self.on;
        self.phase = Phase::Select;
        self.take_wait = 0;
        self.note = if self.on { "running".into() } else { "stopped".into() };
    }

    fn stop(&mut self, why: &'static str) {
        self.on = false;
        self.phase = Phase::Select;
        self.note = why.into();
    }

    /// One step of the trade loop, on the game's ticks.
    fn step(&mut self) {
        let state = screen::state();
        if state.id != "inventory" || state.menu.as_deref() != Some("minecraft:merchant") {
            self.stop("the merchant closed");
            return;
        }
        let Some(trades) = screen::trades() else {
            self.stop("the merchant closed");
            return;
        };
        let inv = inventory::inventory();
        let has = |cost: &TradeStack| holds(&inv.slots, cost);
        let container = inventory::open_container();
        let result_present = container
            .as_ref()
            .is_some_and(|c| c.slots.iter().any(|s| s.slot == RESULT_SLOT));
        match decide(self.phase, &trades, &has, result_present, self.take_wait) {
            Decision::Select(index) => {
                let _ = screen::select_trade(index);
                self.phase = Phase::Take;
                self.take_wait = 0;
            }
            Decision::Take => {
                let _ = inventory_actions::click(Some(RESULT_SLOT), 0, ClickKind::QuickMove);
                let idle = SPEEDS[self.speed];
                if idle == 0 {
                    self.phase = Phase::Select;
                } else {
                    self.phase = Phase::Cooldown;
                    self.cooldown = idle;
                }
            }
            Decision::Wait => match self.phase {
                Phase::Take => self.take_wait += 1,
                Phase::Cooldown => {
                    self.cooldown = self.cooldown.saturating_sub(1);
                    if self.cooldown == 0 {
                        self.phase = Phase::Select;
                    }
                }
                Phase::Select => {}
            },
            Decision::Stop(why) => self.stop(why),
        }
    }

    /// Declares the Speed Trade button while a merchant screen is
    /// open, again only when the set changed.
    fn declare(&mut self) {
        let state = screen::state();
        let merchant = state.id == "inventory" && state.menu.as_deref() == Some("minecraft:merchant");
        let mut buttons = Vec::new();
        if merchant {
            buttons.push(ScreenButton {
                id: BTN_SPEED_TRADE,
                screen: "inventory".into(),
                menu: Some("minecraft:merchant".into()),
                anchor: horizon_mod_sdk::Anchor::ContainerHead,
                label: if self.on {
                    "Speed Trade: on".into()
                } else {
                    "Speed Trade".into()
                },
                icon: None,
            });
        }
        let spelling = buttons
            .iter()
            .map(|b| format!("{}:{}", b.id, b.label))
            .collect::<Vec<_>>()
            .join("|");
        if spelling != self.declared {
            screen::set_buttons(&buttons);
            self.declared = spelling;
        }
    }
}

fn read_index(key: &str, len: usize, default: usize) -> usize {
    let Some(bytes) = storage::get(key) else {
        return default;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return default;
    };
    match text.trim().parse::<usize>() {
        Ok(v) if v < len => v,
        _ => default,
    }
}

impl Mod for FastTrading {
    fn init() -> Result<(Self, Registration), String> {
        let state = FastTrading {
            speed: read_index(SPEED_KEY, SPEEDS.len(), 0),
            on: false,
            phase: Phase::Select,
            cooldown: 0,
            take_wait: 0,
            note: "press Speed Trade on a merchant screen".into(),
            shown_note: String::new(),
            controls_open: false,
            declared: String::new(),
        };
        Ok((state, Registration::new().on_tick().command("fasttrading")))
    }

    fn tick(&mut self, _tick: u64) {
        for event in screen::poll() {
            if let ScreenEvent::Button(id) = event
                && id == BTN_SPEED_TRADE
            {
                self.toggle();
            }
        }
        if self.on {
            self.step();
        }
        self.declare();
        // The controls screen re-opens only when the status it
        // shows has changed (the loop stopped on its own).
        if self.controls_open && self.note != self.shown_note {
            self.open_controls();
        }
    }

    fn command(&mut self, _name: &str, _args: &str) -> Result<Option<String>, String> {
        self.open_controls();
        Ok(Some("FastTrading: controls opened".into()))
    }

    fn ui_settings_opened(&mut self) {
        self.refresh_settings();
    }

    fn ui_select(&mut self, id: u32, _row: u32) {
        match id {
            SET_SPEED => {
                self.cycle_speed();
                self.refresh_settings();
            }
            CTL_SPEED => {
                self.cycle_speed();
            }
            CTL_TOGGLE => {
                self.toggle();
            }
            _ => return,
        }
        if self.controls_open {
            self.open_controls();
        }
    }

    fn ui_closed(&mut self) {
        self.controls_open = false;
    }
}

horizon_mod_sdk::export_mod!(FastTrading);

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_mod_sdk::{TradeList, TradeOffer};

    #[test]
    fn api_level_is_24() {
        assert_eq!(API_LEVEL, 24);
    }

    fn offer(uses: u32, max_uses: u32, available: bool) -> TradeOffer {
        TradeOffer {
            cost_a: TradeStack {
                item: "minecraft:emerald".into(),
                count: 1,
                components: String::new(),
            },
            cost_b: None,
            result: TradeStack {
                item: "minecraft:bread".into(),
                count: 1,
                components: String::new(),
            },
            uses,
            max_uses,
            available,
        }
    }

    fn trades(offer: TradeOffer) -> TradeList {
        TradeList {
            offers: vec![offer],
            selected: Some(0),
        }
    }

    fn rich(_: &TradeStack) -> bool {
        true
    }

    fn broke(_: &TradeStack) -> bool {
        false
    }

    #[test]
    fn a_ready_offer_is_picked() {
        assert_eq!(
            decide(Phase::Select, &trades(offer(0, 12, true)), &rich, false, 0),
            Decision::Select(0)
        );
    }

    #[test]
    fn the_loop_stops_when_the_payments_run_out() {
        assert_eq!(
            decide(Phase::Select, &trades(offer(0, 12, true)), &broke, false, 0),
            Decision::Stop("out of payment")
        );
    }

    #[test]
    fn the_loop_stops_when_the_offer_locks() {
        assert_eq!(
            decide(Phase::Select, &trades(offer(0, 12, false)), &rich, false, 0),
            Decision::Stop("the trade is locked")
        );
        assert_eq!(
            decide(Phase::Select, &trades(offer(12, 12, true)), &rich, false, 0),
            Decision::Stop("the trade is used up")
        );
    }

    #[test]
    fn an_unlimited_offer_never_uses_up() {
        assert_eq!(
            decide(Phase::Select, &trades(offer(99, 0, true)), &rich, false, 0),
            Decision::Select(0)
        );
    }

    #[test]
    fn a_result_still_in_the_slot_stops_the_next_pick() {
        assert_eq!(
            decide(Phase::Select, &trades(offer(0, 12, true)), &rich, true, 0),
            Decision::Stop("no room for the result")
        );
    }

    #[test]
    fn the_take_waits_for_the_staged_result_then_gives_up() {
        assert_eq!(
            decide(Phase::Take, &trades(offer(0, 12, true)), &rich, false, 0),
            Decision::Wait
        );
        assert_eq!(
            decide(Phase::Take, &trades(offer(0, 12, true)), &rich, true, 0),
            Decision::Take
        );
        assert_eq!(
            decide(Phase::Take, &trades(offer(0, 12, true)), &rich, false, TAKE_WAIT_LIMIT),
            Decision::Stop("the trade did not stage")
        );
    }

    #[test]
    fn nothing_selected_stops_the_loop() {
        let mut list = trades(offer(0, 12, true));
        list.selected = None;
        assert_eq!(
            decide(Phase::Select, &list, &rich, false, 0),
            Decision::Stop("no trade selected")
        );
    }

    #[test]
    fn holds_counts_across_stacks_and_components() {
        let slots = vec![
            inventory::InvStack {
                slot: 9,
                id: "minecraft:emerald".into(),
                count: 3,
                damage: 0,
                max_damage: 0,
                components: String::new(),
            },
            inventory::InvStack {
                slot: 10,
                id: "minecraft:emerald".into(),
                count: 4,
                damage: 0,
                max_damage: 0,
                components: String::new(),
            },
        ];
        let cost = TradeStack {
            item: "minecraft:emerald".into(),
            count: 7,
            components: String::new(),
        };
        assert!(holds(&slots, &cost));
        let named = TradeStack {
            components: "[custom_name=x]".into(),
            ..cost
        };
        assert!(!holds(&slots, &named));
    }
}
