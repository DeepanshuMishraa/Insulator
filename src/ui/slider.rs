//! Slider component with drag, click, and keyboard interaction.
//!
//! Provides `SliderState`, `SliderEvent`, `SliderValue`, and `Slider`.

use gpui::{
    App, BorderStyle, Bounds, Context, Div, ElementId, Entity, EventEmitter, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, RenderOnce, StyleRefinement, Styled, Window, canvas, div, point, px,
    quad, rgb, size,
};

use crate::theme::Theme;

/// Represents a single slider value or a range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderValue {
    Single(f32),
    Range(f32, f32),
}

impl SliderValue {
    pub fn start(&self) -> f32 {
        match self {
            SliderValue::Single(v) => *v,
            SliderValue::Range(s, _) => *s,
        }
    }

    pub fn end(&self) -> f32 {
        match self {
            SliderValue::Single(v) => *v,
            SliderValue::Range(_, e) => *e,
        }
    }
}

impl From<f32> for SliderValue {
    fn from(v: f32) -> Self {
        SliderValue::Single(v)
    }
}

impl From<(f32, f32)> for SliderValue {
    fn from((s, e): (f32, f32)) -> Self {
        SliderValue::Range(s, e)
    }
}

impl Default for SliderValue {
    fn default() -> Self {
        SliderValue::Single(0.0)
    }
}

/// Events emitted by [`SliderState`].
#[derive(Clone, Debug, PartialEq)]
pub enum SliderEvent {
    Change(SliderValue),
    Release(SliderValue),
}

/// State of a [`Slider`].
pub struct SliderState {
    min: f32,
    max: f32,
    step: f32,
    value: SliderValue,
    dragging: bool,
    hovered: bool,
    grab_offset: Pixels,
}

impl EventEmitter<SliderEvent> for SliderState {}

impl Default for SliderState {
    fn default() -> Self {
        Self::new()
    }
}

impl SliderState {
    pub fn new() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            value: SliderValue::Single(0.0),
            dragging: false,
            hovered: false,
            grab_offset: Pixels::ZERO,
        }
    }

    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self
    }

    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step.max(f32::EPSILON);
        self
    }

    pub fn default_value(mut self, value: impl Into<SliderValue>) -> Self {
        self.value = value.into();
        self
    }

    pub fn value(&self) -> SliderValue {
        self.value
    }

    pub fn min_value(&self) -> f32 {
        self.min
    }

    pub fn max_value(&self) -> f32 {
        self.max
    }

    pub fn step_value(&self) -> f32 {
        self.step
    }

    pub fn percentage(&self) -> f32 {
        let range = self.max - self.min;
        if range <= 0.0 {
            0.0
        } else {
            ((self.value.start() - self.min) / range).clamp(0.0, 1.0)
        }
    }

    pub fn set_value(&mut self, value: impl Into<SliderValue>, cx: &mut Context<Self>) {
        let raw = value.into();
        let clamped = match raw {
            SliderValue::Single(v) => {
                let stepped = if self.step > 0.0 {
                    (v / self.step).round() * self.step
                } else {
                    v
                };
                SliderValue::Single(stepped.clamp(self.min, self.max))
            }
            SliderValue::Range(s, e) => {
                let s_stepped = (s / self.step).round() * self.step;
                let e_stepped = (e / self.step).round() * self.step;
                SliderValue::Range(
                    s_stepped.clamp(self.min, self.max),
                    e_stepped.clamp(self.min, self.max),
                )
            }
        };
        if self.value != clamped {
            self.value = clamped;
            crate::haptics::trigger(crate::haptics::HapticPattern::LevelChange);
            cx.emit(SliderEvent::Change(self.value));
            cx.notify();
        }
    }
}

/// A stylized, smooth slider component.
#[derive(IntoElement)]
pub struct Slider {
    state: Entity<SliderState>,
    disabled: bool,
    width: Option<Pixels>,
    base: Div,
}

impl Slider {
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            state: state.clone(),
            disabled: false,
            width: None,
            base: div(),
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }
}

impl Styled for Slider {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Slider {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = Theme::current(cx);
        let state_entity = self.state.clone();
        let (min, max, step, percentage, dragging, hovered, value_num) = {
            let s = self.state.read(cx);
            (
                s.min,
                s.max,
                s.step,
                s.percentage(),
                s.dragging,
                s.hovered,
                s.value.start(),
            )
        };
        let disabled = self.disabled;

        let container_id = ElementId::NamedChild(
            std::sync::Arc::new(("slider-container", self.state.entity_id()).into()),
            "box".into(),
        );

        let mut base = self
            .base
            .id(container_id)
            .h(px(24.0))
            .flex_1()
            .flex()
            .items_center()
            .relative()
            .cursor(if disabled {
                gpui::CursorStyle::Arrow
            } else {
                gpui::CursorStyle::PointingHand
            });

        if let Some(w) = self.width {
            base = base.w(w).flex_none();
        }

        if !disabled {
            base = base
                .tab_index(0)
                .focus_visible(|style| style.rounded(px(6.0)).border_1().border_color(theme.accent))
                .on_key_down({
                    let state_entity = state_entity.clone();
                    move |event: &KeyDownEvent, _, cx| {
                        let key = event.keystroke.key.as_str();
                        let delta = match key {
                            "left" | "down" => -step,
                            "right" | "up" => step,
                            "home" => min - value_num,
                            "end" => max - value_num,
                            _ => return,
                        };
                        cx.stop_propagation();
                        let new_val = (value_num + delta).clamp(min, max);
                        state_entity.update(cx, |s, cx| {
                            s.set_value(new_val, cx);
                            cx.emit(SliderEvent::Release(s.value));
                        });
                    }
                });
        }

        let slider_canvas = canvas(
            |_, _, _| (),
            move |bounds: Bounds<Pixels>, _, window: &mut Window, cx: &mut App| {
                let theme = Theme::current(cx);
                let track_height = px(5.0);
                let track_y = bounds.top() + (bounds.size.height - track_height) / 2.0;
                let thumb_radius = px(8.0);
                let travel = (bounds.size.width - thumb_radius * 2.0).max(Pixels::ZERO);
                let thumb_center_x = bounds.left() + thumb_radius + travel * percentage;
                let thumb_center_y = bounds.top() + bounds.size.height / 2.0;

                // 1. Inactive track background
                let track_bg = if disabled {
                    theme.border.opacity(0.4)
                } else if theme.is_dark {
                    rgb(0x353538).into()
                } else {
                    rgb(0xDFE1E5).into()
                };
                window.paint_quad(quad(
                    Bounds::new(
                        point(bounds.left(), track_y),
                        size(bounds.size.width, track_height),
                    ),
                    track_height / 2.0,
                    track_bg,
                    px(0.0),
                    gpui::transparent_black(),
                    BorderStyle::default(),
                ));

                // 2. Active filled track
                let filled_width = if travel > Pixels::ZERO {
                    (thumb_center_x - bounds.left()).max(track_height)
                } else {
                    Pixels::ZERO
                };
                let filled_bg = if disabled {
                    theme.accent.opacity(0.35)
                } else {
                    theme.accent
                };
                window.paint_quad(quad(
                    Bounds::new(
                        point(bounds.left(), track_y),
                        size(filled_width, track_height),
                    ),
                    track_height / 2.0,
                    filled_bg,
                    px(0.0),
                    gpui::transparent_black(),
                    BorderStyle::default(),
                ));

                // 3. Thumb halo ring on hover or dragging
                if !disabled && (hovered || dragging) {
                    let ring_radius = if dragging { px(13.0) } else { px(11.5) };
                    window.paint_quad(quad(
                        Bounds::new(
                            point(thumb_center_x - ring_radius, thumb_center_y - ring_radius),
                            size(ring_radius * 2.0, ring_radius * 2.0),
                        ),
                        ring_radius,
                        theme.accent.opacity(if dragging { 0.3 } else { 0.18 }),
                        px(0.0),
                        gpui::transparent_black(),
                        BorderStyle::default(),
                    ));
                }

                // 4. Thumb drop shadow
                if !disabled {
                    let shadow_radius = thumb_radius;
                    let shadow_offset_y = px(1.0);
                    window.paint_quad(quad(
                        Bounds::new(
                            point(
                                thumb_center_x - shadow_radius,
                                thumb_center_y - shadow_radius + shadow_offset_y,
                            ),
                            size(shadow_radius * 2.0, shadow_radius * 2.0),
                        ),
                        shadow_radius,
                        if theme.is_dark {
                            gpui::transparent_black()
                        } else {
                            rgb(0x000000).into()
                        }
                        .opacity(0.2),
                        px(0.0),
                        gpui::transparent_black(),
                        BorderStyle::default(),
                    ));
                }

                // 5. Thumb body
                let thumb_color = if disabled {
                    theme.text_tertiary
                } else {
                    rgb(0xFFFFFF).into()
                };
                let thumb_border_color = if disabled {
                    theme.border
                } else if theme.is_dark {
                    rgb(0x3A3A3C).into()
                } else {
                    rgb(0xD1D5DB).into()
                };
                window.paint_quad(quad(
                    Bounds::new(
                        point(thumb_center_x - thumb_radius, thumb_center_y - thumb_radius),
                        size(thumb_radius * 2.0, thumb_radius * 2.0),
                    ),
                    thumb_radius,
                    thumb_color,
                    px(1.0),
                    thumb_border_color,
                    BorderStyle::default(),
                ));

                // 6. Thumb center accent dot
                if !disabled {
                    let inner_radius = px(2.5);
                    window.paint_quad(quad(
                        Bounds::new(
                            point(thumb_center_x - inner_radius, thumb_center_y - inner_radius),
                            size(inner_radius * 2.0, inner_radius * 2.0),
                        ),
                        inner_radius,
                        theme.accent,
                        px(0.0),
                        gpui::transparent_black(),
                        BorderStyle::default(),
                    ));
                }

                if disabled {
                    return;
                }

                // Mouse Move Listener
                window.on_mouse_event({
                    let state_entity = state_entity.clone();
                    move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase != gpui::DispatchPhase::Bubble {
                            return;
                        }
                        let is_hovering = bounds.contains(&event.position);
                        let is_dragging = state_entity.read(cx).dragging;

                        if is_dragging {
                            let grab_offset = state_entity.read(cx).grab_offset;
                            let target_x = event.position.x - grab_offset;
                            let p = if travel <= Pixels::ZERO {
                                0.0
                            } else {
                                ((target_x - bounds.left() - thumb_radius) / travel).clamp(0.0, 1.0)
                            };
                            let raw_val = min + p * (max - min);
                            state_entity.update(cx, |s, cx| {
                                s.set_value(raw_val, cx);
                            });
                            window.refresh();
                        } else {
                            let current_hovered = state_entity.read(cx).hovered;
                            if current_hovered != is_hovering {
                                state_entity.update(cx, |s, cx| {
                                    s.hovered = is_hovering;
                                    cx.notify();
                                });
                                window.refresh();
                            }
                        }
                    }
                });

                // Mouse Down Listener
                window.on_mouse_event({
                    let state_entity = state_entity.clone();
                    move |event: &MouseDownEvent, phase, window, cx| {
                        if phase != gpui::DispatchPhase::Bubble
                            || event.button != MouseButton::Left
                            || !bounds.contains(&event.position)
                        {
                            return;
                        }
                        let on_thumb = (event.position.x - thumb_center_x).abs()
                            <= thumb_radius + px(4.0)
                            && (event.position.y - thumb_center_y).abs() <= thumb_radius + px(4.0);
                        let grab_offset = if on_thumb {
                            event.position.x - thumb_center_x
                        } else {
                            Pixels::ZERO
                        };
                        let target_x = event.position.x - grab_offset;
                        let p = if travel <= Pixels::ZERO {
                            0.0
                        } else {
                            ((target_x - bounds.left() - thumb_radius) / travel).clamp(0.0, 1.0)
                        };
                        let raw_val = min + p * (max - min);
                        state_entity.update(cx, |s, cx| {
                            s.dragging = true;
                            s.grab_offset = grab_offset;
                            s.set_value(raw_val, cx);
                        });
                        window.refresh();
                    }
                });

                // Mouse Up Listener
                window.on_mouse_event({
                    let state_entity = state_entity.clone();
                    move |event: &MouseUpEvent, phase, window, cx| {
                        if phase != gpui::DispatchPhase::Bubble || event.button != MouseButton::Left
                        {
                            return;
                        }
                        let was_dragging = state_entity.read(cx).dragging;
                        if was_dragging {
                            state_entity.update(cx, |s, cx| {
                                s.dragging = false;
                                s.grab_offset = Pixels::ZERO;
                                cx.emit(SliderEvent::Release(s.value));
                                cx.notify();
                            });
                            crate::haptics::trigger(crate::haptics::HapticPattern::Alignment);
                            window.refresh();
                        }
                    }
                });
            },
        );

        base.child(slider_canvas.size_full())
    }
}
