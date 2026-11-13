//! Zeus's own parts of a message's bubble: arrival, status motion and inline editing.

use super::*;

/// Two fingers swiped across a message the way Settings give a reply: the
/// row follows them with a rubber band while a reply icon grows in the space
/// it leaves, and letting go past the threshold starts the reply. Short of
/// it the row eases back.
pub(super) fn swipe_to_reply(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    start: egui::layers::ShapeIdx,
    rect: Rect,
    actions: &mut Vec<Action>,
) {
    use crate::settings::{SwipeAction, SwipeSlot};
    let ctx = ui.ctx().clone();
    let clip = ui.clip_rect();
    let area = Rect::from_x_y_ranges(clip.x_range(), rect.expand2(vec2(0.0, 4.0)).y_range());
    let pull = crate::ui::swipe::over(&ctx, area);
    let reply = |right| {
        (view.swipes.get(SwipeSlot::of(true, right)) == SwipeAction::Reply && view.chat.can_send())
            .then_some(Icon::Reply)
    };
    if let Some(pull) = pull
        && pull.fired
        && reply(pull.right).is_some()
    {
        actions.push(Action::Reply(message.id.clone()));
    }
    let lane = Rect::from_x_y_ranges(
        (rect.left() - 12.0).max(clip.left())..=(rect.right() + 12.0).min(clip.right()),
        rect.y_range(),
    );
    let id = bubble_id(&view.chat.id, &message.id);
    let pulled = crate::ui::swipe::slide_row(
        ui,
        id.with("swipe"),
        start,
        lane,
        pull,
        reply,
        &view.palette,
    );
    // The hover buttons would ride along into the badge's space.
    ctx.data_mut(|data| data.insert_temp(id.with("swiping"), pulled > 0.0));
}

/// A sideways swipe across the open chat whose action is on the whole chat
/// (a reply is the message rows'): a badge slides in at the side the
/// fingers come from, and letting go past the threshold acts.
pub(super) fn swipe_chat(
    app: &mut App,
    ui: &egui::Ui,
    chat: &Chat,
    area: Rect,
    actions: &mut Vec<Action>,
) {
    use crate::settings::SwipeAction;
    let pull = crate::ui::swipe::over(ui.ctx(), area);
    if let Some(pull) = pull.filter(|pull| pull.fired) {
        let action = crate::ui::swipe::action(app, true, pull.right);
        if action != SwipeAction::Reply {
            actions.extend(crate::ui::swipe::actions(app, action, chat, None));
        }
    }
    let whole = |right| {
        let action = crate::ui::swipe::action(app, true, right);
        (action != SwipeAction::Reply && crate::ui::swipe::available(app, action, chat, false))
            .then(|| crate::ui::swipe::icon(action, chat))
    };
    let id = egui::Id::new(("swipe-open-chat", &chat.id));
    crate::ui::swipe::edge_badge(ui, id, area, pull, whole, &app.palette);
}

/// Draws a message row. A message just sent rises out of the composer into
/// its place, as iMessage's does; one just received grows out of the typing
/// bubble, or pops out of its tail corner, as does a send that left no
/// flight.
pub(super) fn bubble_arriving(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    album: Option<&[Message]>,
    place: RunPlace,
    reduce_motion: bool,
    actions: &mut Vec<Action>,
) -> Option<egui::Response> {
    // Under a second, only on a send or a receipt: unlike the looping
    // animations `motion` paces, these need no screen-reader exception (and
    // on Windows any accessibility client, not only a reader, sets it).
    let fresh = view.animate && (crate::util::now() - message.timestamp).abs() <= ARRIVAL_FRESH;
    if !fresh {
        return bubble_row(ui, view, message, album, place, reduce_motion, actions);
    }
    let now = ui.input(|input| input.time);
    let layer = ui.layer_id();
    let start = ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    if message.from_me {
        // Rows draw oldest first and flights queue oldest first, so each
        // unclaimed flight takes the next of our messages sent since it.
        let flight = ui.ctx().data_mut(|data| {
            let flights = data.get_temp_mut_or_default::<Vec<SendFlight>>(send_flight_id());
            let claimed = |flights: &[SendFlight]| {
                flights
                    .iter()
                    .position(|f| f.message.as_ref().is_some_and(|(id, _)| *id == message.id))
            };
            if claimed(flights).is_none()
                && let Some(free) = flights.iter_mut().find(|f| {
                    f.chat == view.chat.id
                        && f.message.is_none()
                        && now - f.sent < 2.0
                        && message.timestamp >= f.wall - 1
                })
            {
                free.message = Some((message.id.clone(), now));
            }
            claimed(flights).map(|index| flights[index].clone())
        });
        if let Some((flight, landed)) = flight.and_then(|flight| {
            let landed = flight.message.as_ref()?.1;
            Some((flight, landed))
        }) {
            let response = bubble_row(ui, view, message, album, place, reduce_motion, actions);
            let elapsed = now - landed;
            if send_settled(elapsed) {
                return response;
            }
            let Some(rect) = ui.ctx().data(|data| {
                data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("rect"))
            }) else {
                return response;
            };
            // iMessage's send: the bubble keeps its own size and leaves with
            // its text where the composer's text sat, then rises on a spring
            // that passes its place a little while it slides across on a
            // critically damped one.
            let margin = bubble_margin(&view.palette, message, false);
            let leave = vec2(
                flight.from.left() - (rect.left() + f32::from(margin.left)),
                flight.from.bottom() - (rect.bottom() - f32::from(margin.bottom)),
            );
            let across = SEND_ACROSS.value(elapsed);
            let up = SEND_RISE.value(elapsed);
            // The list rises to meet a new last message; aim where the row is
            // drawn now, not where it ends, or the bubble passes it.
            let gliding = ui
                .ctx()
                .data(|data| data.get_temp::<Glide>(Glide::id(&view.chat.id)))
                .map_or(0.0, |glide| glide.shift(now, 0));
            let offset = vec2(
                leave.x * (1.0 - across),
                gliding + (leave.y - gliding) * (1.0 - up),
            );
            let transform = egui::emath::TSTransform::from_translation(offset);
            // It leaves in the field's colours and turns into its own over
            // the first moments: the field's version lies over it, fading.
            let veil = 1.0 - theme::ease_out((elapsed / SEND_TINT_SECONDS) as f32);
            let own_text = view.palette.on_bubble(true).text;
            // Lifted onto a layer above the chat, so it can cross the composer
            // and the list's edge on its way, and the list's own glide leaves it be.
            let flying = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("send-flight"));
            let clip = ui.ctx().content_rect();
            ui.ctx().graphics_mut(|graphics| {
                let list = graphics.entry(layer);
                let end = list.next_idx();
                let shapes: Vec<egui::Shape> = list
                    .all_entries()
                    .skip(start.0)
                    .take(end.0 - start.0)
                    .map(|clipped| clipped.shape.clone())
                    .collect();
                for index in start.0..end.0 {
                    list.reset_shape(egui::layers::ShapeIdx(index));
                }
                let veiled = if veil > 0.0 {
                    field_veil(&shapes, rect, (flight.field, flight.ink), own_text, veil)
                } else {
                    Vec::new()
                };
                let above = graphics.entry(flying);
                for mut shape in shapes.into_iter().chain(veiled) {
                    shape.transform(transform);
                    above.add(clip, shape);
                }
            });
            ui.ctx().request_repaint();
            return response;
        }
    }
    // Received, or a send that left no flight. The first time it is drawn
    // starts its spring, unless it was already there when the chat itself
    // first appeared.
    let shown = ui
        .ctx()
        .data_mut(|data| *data.get_temp_mut_or(chat_shown_id(&view.chat.id), now));
    let id = egui::Id::new(("landing", &view.chat.id, &message.id));
    // A received message whose typing bubble was drawn a frame ago grows out
    // of that bubble. Taken once, when it lands: the typing bubble is gone
    // after that.
    let typing = (!message.from_me)
        .then(|| {
            ui.ctx()
                .data(|data| data.get_temp::<TypingSeen>(typing_bubble_id(&view.chat.id)))
        })
        .flatten()
        .filter(|seen| now - seen.drawn <= TYPING_HANDOFF)
        .map(|seen| seen.rect);
    let (landed, typing) = ui
        .ctx()
        .data_mut(|data| *data.get_temp_mut_or_insert_with(id, || (now, typing)));
    let elapsed = now - landed;
    if POP.settled(elapsed) || landed <= shown {
        return bubble_row(ui, view, message, album, place, reduce_motion, actions);
    }
    let row = ui.scope(|ui| {
        if typing.is_none() {
            ui.multiply_opacity(pop_opacity(elapsed));
        }
        bubble_row(ui, view, message, album, place, reduce_motion, actions)
    });
    // From the tail's corner of the bubble itself, not of the row around it
    // with its picture and reactions.
    let bubble = ui
        .ctx()
        .data(|data| data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("rect")))
        .unwrap_or(row.response.rect);
    let pivot = if message.from_me {
        bubble.right_bottom()
    } else {
        bubble.left_bottom()
    };
    let transform = if let Some(typing) = typing {
        // Out of the typing bubble: from its bottom left and its height,
        // fully opaque, springing to the row's own place and size.
        let p = POP.value(elapsed);
        let from_scale = (typing.height() / row.response.rect.height().max(1.0)).clamp(0.5, 1.0);
        let scale = from_scale + (1.0 - from_scale) * p;
        let offset = (typing.left_bottom() - pivot) * (1.0 - p);
        egui::emath::TSTransform::from_translation(offset) * scale_about(pivot, scale)
    } else {
        // iMessage's receive: it pops out of its tail corner, from a little
        // over half its size to a hair past its own, and settles.
        scale_about(pivot, pop_scale(elapsed))
    };
    ui.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(layer);
        let end = list.next_idx();
        for index in start.0..end.0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                clipped.shape.transform(transform);
            });
        }
    });
    ui.ctx().request_repaint();
    row.inner
}

/// When the open chat's transcript was first drawn: messages already there
/// then appear in place.
pub(super) fn chat_shown_id(chat: &str) -> egui::Id {
    in_window(egui::Id::new(("chat-shown", chat)))
}

/// The composer field's version of a sent bubble, to lay over the bubble
/// at `opacity` as it leaves: its backdrop in the field's `fill`, and its
/// body text, in the bubble's own `text` colour, in the field's `ink`.
/// Fading it out turns the field into the bubble. The time, ticks and
/// scrim are left to the bubble beneath.
pub(super) fn field_veil(
    shapes: &[egui::Shape],
    bubble: Rect,
    (fill, ink): (Color32, Color32),
    text: Color32,
    opacity: f32,
) -> Vec<egui::Shape> {
    // The backdrop is the first shape the size of the bubble: its tail may
    // reach a few points past it, a scrim or shadow further, blurred.
    let near = bubble.expand(8.0);
    let core = bubble.shrink(1.0);
    let mut backdrop = false;
    let mut veil = Vec::new();
    for shape in shapes {
        let mut shape = shape.clone();
        match &shape {
            // The body, not the time beside it.
            egui::Shape::Text(body)
                if body.fallback_color == text
                    || body.override_text_color == Some(text)
                    || body
                        .galley
                        .job
                        .sections
                        .iter()
                        .any(|section| section.format.color == text) =>
            {
                egui::epaint::shape_transform::adjust_colors(&mut shape, move |colour| {
                    if *colour == text {
                        *colour = ink;
                    }
                });
            }
            // Emoji are pictures painted over the text, the same in both.
            egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {}
            _ if !backdrop && !blurred(&shape) && {
                let bounds = shape.visual_bounding_rect();
                near.contains_rect(bounds) && bounds.contains_rect(core)
            } =>
            {
                backdrop = true;
                egui::epaint::shape_transform::adjust_colors(&mut shape, move |colour| {
                    if *colour != Color32::PLACEHOLDER && colour.a() > 0 {
                        *colour = fill.gamma_multiply(f32::from(colour.a()) / 255.0);
                    }
                });
            }
            _ => continue,
        }
        egui::epaint::shape_transform::adjust_colors(&mut shape, move |colour| {
            if *colour != Color32::PLACEHOLDER {
                *colour = colour.gamma_multiply(opacity);
            }
        });
        veil.push(shape);
    }
    veil
}

/// Whether a shape is a soft one, a shadow or scrim, rather than a solid.
pub(super) fn blurred(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::Rect(rect) => rect.blur_width > 0.0,
        egui::Shape::Vec(shapes) => shapes.iter().any(blurred),
        _ => false,
    }
}

/// Draws a message row and returns its bubble response for scrolling.
pub(super) fn bubble_row(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    album: Option<&[Message]>,
    place: RunPlace,
    reduce_motion: bool,
    actions: &mut Vec<Action>,
) -> Option<egui::Response> {
    let actions_before = actions.len();
    let RunPlace {
        show_sender,
        show_avatar,
        tailed,
        first_in_run,
        last_in_run,
        last_outgoing,
    } = place;
    let own = message.from_me;
    // Greys that read on the panel can vanish on the bubble; use its own.
    let view = &View {
        palette: view.palette.on_bubble(own),
        ..*view
    };
    let with_avatar = !own && shows_sender_pictures(view.chat);
    let carousel = matches!(&message.content, Content::Interactive { card: Some(card), .. } if !card.carousel.is_empty());
    let share = if carousel {
        0.95
    } else if view.palette.bubbles == theme::BubbleStyle::Messages {
        MESSAGES_WIDTH_SHARE
    } else if view.palette.bubbles == theme::BubbleStyle::Nothing {
        0.70
    } else {
        WHATSAPP_WIDTH_SHARE
    };
    let max_width = ((ui.available_width() * share).min(if carousel { 920.0 } else { 560.0 })
        - if with_avatar {
            SENDER_AVATAR + 8.0
        } else {
            0.0
        })
    .max(0.0);
    // Register the empty strip beside the bubble from its previous rect, before
    // the row, so the avatar, the bubble, and the reactions win clicks, and the
    // text wins drags. A drag that starts here, off the text, sweeps messages
    // into a selection (#246).
    let id = bubble_id(&view.chat.id, &message.id);
    let previous = ui.ctx().data(|data| data.get_temp::<Rect>(id.with("rect")));
    if let Some(rect) = previous {
        let strip = Rect::from_x_y_ranges(ui.max_rect().x_range(), rect.y_range());
        let strip = ui.interact(strip, id.with("row"), Sense::click_and_drag());
        if strip.clicked() {
            actions.push(Action::FocusComposer);
        }
        // A drag from the bubble's own padding or media, under which this
        // strip runs too, picks the message up instead.
        if strip.drag_started()
            && !crate::ui::carry::lift_on_drag(ui.ctx(), &view.chat.id, &message.id)
        {
            actions.push(Action::SweepMessages {
                anchor: message.id.clone(),
                to: message.id.clone(),
            });
        }
        reply_on_double_click(&strip, view, message, actions);
    }
    let mut response = None;
    ui.with_layout(
        Layout::top_down(if own { Align::Max } else { Align::Min }),
        |ui| {
            if with_avatar {
                ui.horizontal_top(|ui| {
                    let (space, _) =
                        ui.allocate_exact_size(Vec2::splat(SENDER_AVATAR), Sense::hover());
                    // Keep bubble content vertically laid out inside the row.
                    ui.vertical(|ui| {
                        response = Some(bubble_frame(
                            ui,
                            view,
                            message,
                            album,
                            show_sender,
                            tailed,
                            first_in_run,
                            last_in_run,
                            max_width,
                            actions,
                        ));
                    });
                    if !show_avatar {
                        return;
                    }
                    // Messages and Nothing put the avatar beside the run's last
                    // message, level with its bottom; WhatsApp beside the
                    // first, at its top.
                    let rect = match &response {
                        Some(bubble)
                            if matches!(
                                view.palette.bubbles,
                                theme::BubbleStyle::Messages | theme::BubbleStyle::Nothing
                            ) =>
                        {
                            Rect::from_min_size(
                                pos2(
                                    space.left(),
                                    (bubble.rect.bottom() - SENDER_AVATAR).max(space.top()),
                                ),
                                Vec2::splat(SENDER_AVATAR),
                            )
                        }
                        _ => space,
                    };
                    let avatar = ui.interact(rect, id.with("avatar"), Sense::CLICK);
                    theme::focus_outline(ui, avatar.id, rect, SENDER_AVATAR / 2.0);
                    if avatar
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        actions.push(Action::OpenInfo(message.sender.clone()));
                    }
                    if ui.is_rect_visible(rect) {
                        let name = (view.names_or)(&message.sender, message.sender_name.as_deref());
                        widgets::paint_avatar(
                            ui,
                            &view.palette,
                            rect,
                            name.trim_start_matches('~'),
                            &message.sender,
                            view.avatars
                                .get(&message.sender)
                                .and_then(|picture| picture.as_deref()),
                        );
                    }
                });
            } else {
                response = Some(bubble_frame(
                    ui,
                    view,
                    message,
                    album,
                    show_sender,
                    tailed,
                    first_in_run,
                    last_in_run,
                    max_width,
                    actions,
                ));
            }
            // An album row carries every member's reactions, each set
            // reacting to its own item.
            let members = album.unwrap_or(std::slice::from_ref(message));
            if members.iter().any(|member| !member.reactions.is_empty()) {
                // Off the spacing scale on purpose: the chips overlap the
                // bubble's lower edge.
                ui.add_space(-5.0);
                // Chips hang off the bubble's far side: the right of a
                // received message, the left of one we sent.
                let bubble_width = ui
                    .ctx()
                    .data(|data| data.get_temp::<Rect>(id.with("rect")))
                    .map_or(max_width, |rect| rect.width());
                let lead = if with_avatar {
                    SENDER_AVATAR + 8.0
                } else {
                    0.0
                };
                let layout = if own {
                    Layout::left_to_right(Align::Center)
                } else {
                    Layout::right_to_left(Align::Center)
                };
                ui.allocate_ui_with_layout(vec2(lead + bubble_width, 0.0), layout, |ui| {
                    // A right-to-left row lays its first item last.
                    let mut shown = members
                        .iter()
                        .filter(|member| !member.reactions.is_empty())
                        .collect::<Vec<_>>();
                    if !own {
                        shown.reverse();
                    }
                    for member in shown {
                        reactions(ui, view, member, actions);
                    }
                });
                ui.add_space(4.0);
            }
            // Members without a pill record that, so the first reaction
            // that arrives live pops like any later change.
            for member in members.iter().filter(|member| member.reactions.is_empty()) {
                note_no_reactions(ui, view, member);
            }
        },
    );
    // A picture without a caption already carries its time over its corner:
    // a second one under the bubble would say it twice.
    let time_on_picture = album.is_none() && time_over_picture(message);
    if !view.time_inside && !view.selecting && !time_on_picture {
        let bubble_rect = ui
            .ctx()
            .data(|data| data.get_temp::<Rect>(id.with("rect")))
            .or_else(|| response.as_ref().map(|response| response.rect));
        // A photo, video or link card opens itself: only the text and the
        // bubble around it show the time.
        let media = [id.with("picture"), id.with("preview"), id.with("quote")]
            .map(|key| ui.ctx().data(|data| data.get_temp::<Rect>(key)));
        let clicked_inside = bubble_rect.is_some_and(|rect| {
            ui.input(|input| {
                input.pointer.button_clicked(egui::PointerButton::Primary)
                    && input.pointer.interact_pos().is_some_and(|position| {
                        rect.contains(position)
                            && !media.iter().flatten().any(|media| media.contains(position))
                    })
            })
        });
        let metadata_id = id.with("metadata-visible");
        let default_visible = last_outgoing && message.from_me;
        let mut visible = ui
            .ctx()
            .data(|data| data.get_temp::<bool>(metadata_id))
            .unwrap_or(default_visible);
        let on_media = ui.input(|input| {
            input.pointer.interact_pos().is_some_and(|position| {
                media.iter().flatten().any(|media| media.contains(position))
            })
        });
        // A click that opened something (a link, a quote's message, a file,
        // a sender) did what it was for; the time stays as it was.
        let opened = actions[actions_before..].iter().any(|action| {
            matches!(
                action,
                Action::OpenUrl(_)
                    | Action::ScrollTo(_)
                    | Action::OpenMessage { .. }
                    | Action::PreviewImage { .. }
                    | Action::PreviewSticker(_)
                    | Action::OpenFile(_)
                    | Action::OpenInfo(_)
                    | Action::PlayVideo { .. }
            )
        });
        let bubble_clicked = !opened
            && (clicked_inside
                || (!on_media && response.as_ref().is_some_and(egui::Response::clicked)));
        if bubble_clicked {
            visible = !visible;
            ui.ctx()
                .data_mut(|data| data.insert_temp(metadata_id, visible));
        }
        let animation_time = if reduce_motion {
            0.0
        } else {
            theme::MOTION_QUICK
        };
        let progress = theme::animate(
            ui.ctx(),
            metadata_id.with("reveal"),
            visible,
            animation_time,
        );
        if progress > 0.0 {
            let display_message = album.and_then(|members| members.last()).unwrap_or(message);
            if reference_timestamp(
                ui,
                view,
                display_message,
                id.with("metadata-row"),
                progress,
                bubble_rect,
            )
            .clicked()
                && !bubble_clicked
            {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(metadata_id, false));
            }
        }
    }
    response
}

/// The message time and delivery state sit just below a bubble when selected.
pub(super) fn reference_timestamp(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    id: egui::Id,
    progress: f32,
    bubble: Option<Rect>,
) -> egui::Response {
    let own = message.from_me;
    const SPACING: f32 = 4.0;
    const ROW_HEIGHT: f32 = 16.0;
    let full_height = SPACING + ROW_HEIGHT;
    let reveal_height = full_height * progress.clamp(0.0, 1.0);
    let available = ui.available_rect_before_wrap();
    let slot_top = ui.cursor().top();
    let slot = Rect::from_min_size(
        pos2(available.left(), slot_top),
        vec2(available.width(), reveal_height),
    );
    ui.allocate_space(vec2(available.width(), reveal_height));

    // Keep the metadata at its final position and reveal it through a clip
    // that grows with the row. The transcript's ordinary layout then moves
    // subsequent messages in step, instead of jumping by the whole row.
    // Under the bubble, ending at its right edge on either side. A bubble
    // narrower than the time and ticks (a two-letter reply) lets the row reach
    // past its left edge instead of cutting the time short.
    let time = crate::util::clock(message.timestamp);
    let shown = shown_delivery(view, message);
    let status = message_status_text(view, message);
    let needed = match &status {
        Some(text) => reference_timestamp_width(ui, text, false),
        None => reference_timestamp_width(ui, &time, own),
    };
    let x = bubble.map_or(available.x_range(), |bubble| bubble.x_range());
    let x = widen_to(x, needed, available.x_range());
    let row_rect = Rect::from_x_y_ranges(x, slot_top + SPACING..=slot_top + SPACING + ROW_HEIGHT);
    let clip = Rect::from_min_max(slot.min, pos2(slot.right(), slot.top() + reveal_height))
        .intersect(ui.clip_rect());
    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(row_rect)
            .layout(Layout::top_down(Align::Max)),
    );
    row_ui.set_clip_rect(clip);
    row_ui.multiply_opacity(progress);
    row_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if let Some(text) = &status {
            // A word in place of the time and ticks: the time only when the
            // state came well after the message.
            let font = theme::regular(10.0);
            let colour = status_colour(view, shown, view.palette.secondary.gamma_multiply(0.82));
            let width = ui
                .painter()
                .layout_no_wrap(text.clone(), font.clone(), colour)
                .size()
                .x;
            let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
            paint_status_word(
                ui,
                view,
                message,
                shown,
                text,
                (rect.right(), rect.center().y),
                &font,
                colour,
            );
        } else if own {
            // Noted while it is still sending, so the word that follows
            // eases in.
            let _ = status_motion(ui, view, message, shown);
            match message.status {
                Delivery::Pending => {
                    widgets::rich_text(
                        ui,
                        "•••",
                        theme::semibold(theme::TEXT_CAPTION),
                        view.palette.secondary,
                    );
                }
                Delivery::Failed => {
                    widgets::rich_text(
                        ui,
                        "✗",
                        theme::semibold(theme::TEXT_CAPTION),
                        view.palette.danger,
                    );
                }
                _ => {}
            }
            widgets::rich_text(
                ui,
                &time,
                theme::regular(theme::TEXT_MICRO),
                view.palette.secondary.gamma_multiply(0.82),
            );
        } else {
            widgets::rich_text(
                ui,
                &time,
                theme::regular(theme::TEXT_MICRO),
                view.palette.secondary.gamma_multiply(0.82),
            );
        }
    });
    ui.interact(row_rect.intersect(clip), id, Sense::click())
}

/// The room the time and delivery mark under a bubble take, with their gap.
pub(super) fn reference_timestamp_width(ui: &egui::Ui, time: &str, own: bool) -> f32 {
    let time = ui
        .painter()
        .layout_no_wrap(
            time.to_owned(),
            theme::regular(theme::TEXT_MICRO),
            Color32::WHITE,
        )
        .size()
        .x;
    // The mark is 14 points of ticks, or "•••" / "✗" while sending or failed.
    time + if own { 14.0 + 4.0 } else { 0.0 } + 2.0
}

/// `span` grown to the left to be `needed` wide, or shifted right when that
/// would leave `bounds`. The right edge only moves for a span narrower than
/// `needed` that sits against the left edge of `bounds`.
pub(super) fn widen_to(span: egui::Rangef, needed: f32, bounds: egui::Rangef) -> egui::Rangef {
    if span.span() >= needed {
        return span;
    }
    let left = (span.max - needed).max(bounds.min);
    egui::Rangef::new(left, (left + needed).max(span.max))
}

/// How long a delivery tick's change runs.
pub(super) const TICK_SECONDS: f64 = 0.14;

/// Whether a status moved on since it was last drawn. No earlier status, or a
/// failure, is not an advance.
pub(super) fn status_advanced(previous: Option<Delivery>, now: Delivery) -> bool {
    matches!(previous, Some(before) if before != now) && now != Delivery::Failed
}

/// How far a status change has got, from 0 to 1, and the status it moved
/// on from; `None` once it has run its course, with a screen reader (which
/// sees the change at once), or when this frame is not one that follows a
/// change. Every draw of a message's status calls it, so it also keeps the
/// status the last frame saw.
pub(super) fn status_motion(
    ui: &egui::Ui,
    view: &View<'_>,
    message: &Message,
    shown: Delivery,
) -> Option<(f32, Delivery)> {
    let now = ui.input(|input| input.time);
    let status_id = egui::Id::new(("tick-status", &view.chat.id, &message.id));
    let motion_id = egui::Id::new(("tick-motion", &view.chat.id, &message.id));
    let calm = crate::motion::reduced(ui.ctx());
    // Only a status seen on the frame before counts as earlier: a bubble
    // scrolled back into view after a change shows its status as it is.
    let frame = ui.ctx().cumulative_frame_nr();
    let previous = seen_last_frame(
        ui.ctx()
            .data(|data| data.get_temp::<(u64, Delivery)>(status_id)),
        frame,
    );
    let advanced = status_advanced(previous, shown);
    ui.ctx().data_mut(|data| {
        data.insert_temp(status_id, (frame, shown));
        if advanced && !calm {
            data.insert_temp(motion_id, (now, previous.unwrap_or(shown)));
        }
    });
    let motion: Option<(f64, Delivery)> = ui.ctx().data(|data| data.get_temp(motion_id));
    motion
        .filter(|(start, _)| now - start < TICK_SECONDS && !calm)
        .map(|(start, before)| (((now - start) / TICK_SECONDS) as f32, before))
}

/// How far the old and new status word slide as one gives way to the other.
pub(super) const STATUS_SLIDE: f32 = 4.0;

/// Our message's time and delivery state show the time only for a state that
/// came this long (seconds) after the message was sent: a reply that is read
/// at once says just "Read".
pub(super) const STATUS_STAMP_AFTER: i64 = 5 * 60;

/// When a state was reached, if that was late enough to say: more than
/// [`STATUS_STAMP_AFTER`] seconds after the message was sent. A message
/// still only sent that long after it left has no later state to name, so it
/// gives the time it was sent.
pub(super) fn status_stamp(message: &Message, shown: Delivery, now: i64) -> Option<i64> {
    let sent = message.timestamp;
    let at = match shown {
        Delivery::Sent => return (now - sent > STATUS_STAMP_AFTER).then_some(sent),
        Delivery::Delivered => message.delivered_at?,
        Delivery::Read | Delivery::Played => message.read_at?,
        Delivery::None | Delivery::Pending | Delivery::Failed => return None,
    };
    (at - sent > STATUS_STAMP_AFTER).then_some(at)
}

/// What our message's status says in place of ticks, such as "Read" or
/// "Delivered • 11:01 AM"; `None` for a message that is not ours and for the
/// states that keep their glyph (sending, failed).
pub(super) fn status_text(
    locale: crate::i18n::Locale,
    message: &Message,
    shown: Delivery,
    now: i64,
) -> Option<String> {
    if !message.from_me {
        return None;
    }
    let word = status_word(locale, shown)?;
    Some(match status_stamp(message, shown, now) {
        Some(at) => format!("{word} • {}", crate::util::clock(at)),
        None => word,
    })
}

/// [`status_text`] as a view draws it.
pub(super) fn message_status_text(view: &View<'_>, message: &Message) -> Option<String> {
    status_text(
        view.locale,
        message,
        shown_delivery(view, message),
        crate::util::now(),
    )
}

/// The colour of a status word: the read colour once it is read, `plain`
/// otherwise.
pub(super) fn status_colour(view: &View<'_>, shown: Delivery, plain: Color32) -> Color32 {
    match shown {
        Delivery::Read | Delivery::Played => view.palette.read,
        _ => plain,
    }
}

/// Draws our message's status `text` with its right edge at `right`, centred
/// on `y`. When the state has just moved on the new word eases in, sliding up
/// into place, as the old one eases out and away.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_status_word(
    ui: &egui::Ui,
    view: &View<'_>,
    message: &Message,
    shown: Delivery,
    text: &str,
    (right, y): (f32, f32),
    font: &egui::FontId,
    colour: Color32,
) {
    let laid = |text: &str, colour: Color32| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font.clone(), colour)
    };
    let Some((progress, before)) = status_motion(ui, view, message, shown) else {
        let galley = laid(text, colour);
        let at = pos2(right - galley.size().x, y - galley.size().y / 2.0);
        ui.painter().galley(at, galley, colour);
        return;
    };
    let t = theme::ease_out(progress);
    let galley = laid(text, colour.gamma_multiply(t));
    let at = pos2(
        right - galley.size().x,
        y - galley.size().y / 2.0 + STATUS_SLIDE * (1.0 - t),
    );
    ui.painter().galley(at, galley, colour);
    if let Some(old) = status_word(view.locale, before) {
        let old_colour = status_colour(view, before, colour);
        let galley = laid(&old, old_colour.gamma_multiply(1.0 - t));
        let at = pos2(
            right - galley.size().x,
            y - galley.size().y / 2.0 - STATUS_SLIDE * t,
        );
        ui.painter().galley(at, galley, old_colour);
    }
    ui.ctx().request_repaint();
}

/// Draws the delivery ticks, in `plain` for the states that are not read or
/// failed. A live advance scales the new glyph in about its centre with a
/// fade, and a change to read blends the colour.
pub(super) fn tick_motion(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    shown: Delivery,
    rect: Rect,
    plain: Color32,
) {
    let Some((progress, before)) = status_motion(ui, view, message, shown) else {
        widgets::ticks_in(ui, &view.palette, rect, shown, plain);
        return;
    };
    let t = theme::ease_out(progress);
    let layer = ui.layer_id();
    let first_shape = ui.ctx().graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(egui::layers::ShapeIdx(0), |list| list.next_idx())
    });
    let same_glyph = matches!(
        before,
        Delivery::Delivered | Delivery::Read | Delivery::Played
    ) && matches!(
        shown,
        Delivery::Delivered | Delivery::Read | Delivery::Played
    );
    if same_glyph {
        // Only the colour moves, from the old state's to the new one's.
        let palette = &view.palette;
        let colour = |status: Delivery| match status {
            Delivery::Read | Delivery::Played => palette.read,
            _ => plain,
        };
        let blended = colour(before).lerp_to_gamma(colour(shown), t);
        widgets::ticks_in(ui, palette, rect, Delivery::Sent, blended);
    } else {
        let old_opacity = ui.opacity();
        ui.multiply_opacity(t);
        widgets::ticks_in(ui, &view.palette, rect, shown, plain);
        ui.set_opacity(old_opacity);
        let scale = 0.7 + 0.3 * t;
        let pivot = rect.center();
        let transform = egui::emath::TSTransform::from_translation(pivot.to_vec2())
            * egui::emath::TSTransform::from_scaling(scale)
            * egui::emath::TSTransform::from_translation(-pivot.to_vec2());
        ui.ctx().graphics_mut(|graphics| {
            let list = graphics.entry(layer);
            let end = list.next_idx();
            for index in first_shape.0..end.0 {
                list.mutate_shape(egui::layers::ShapeIdx(index), |clipped| {
                    clipped.shape.transform(transform);
                });
            }
        });
    }
    ui.ctx().request_repaint();
}

/// Where the message being edited in its own bubble is kept: its id and the
/// text so far. The state belongs to the view, so nothing reaches the app
/// until the edit is saved.
pub(super) fn inline_edit_slot(chat: &str) -> egui::Id {
    egui::Id::new(("inline-edit", chat))
}

/// Whether a double-click edits this message in its bubble: our own text,
/// inside the edit window. A mentioning message edits through the composer,
/// which keeps its mentions.
pub(super) fn can_edit_inline(view: &View<'_>, message: &Message) -> bool {
    message.from_me
        && matches!(message.content, Content::Text { .. })
        && message.mentions.is_empty()
        && view.now - message.timestamp <= crate::app::EDIT_WINDOW.as_secs() as i64
}

/// Opens the message's text for editing in its bubble.
pub(super) fn start_inline_edit(ctx: &egui::Context, view: &View<'_>, message: &Message) {
    let Content::Text { text, .. } = &message.content else {
        return;
    };
    // The body's size as it was drawn, so the bubble keeps it until the
    // text outgrows it.
    let body =
        ctx.data(|data| data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("body")));
    let slot = inline_edit_slot(&view.chat.id);
    ctx.data_mut(|data| {
        data.insert_temp(slot, (message.id.clone(), text.clone()));
        data.insert_temp(
            slot.with("size"),
            body.map_or(Vec2::ZERO, |body| body.size()),
        );
    });
    ctx.request_repaint();
}

/// How wide the editor of `text` is: as wide as the body was drawn, or the
/// text's own width once it outgrows that, with the time on its last line
/// when it fits there as it does beside the text. Never past `width`.
/// Returns the width and whether the time fits on the last line.
///
/// The rule is [`rich_body`]'s, so the first frame of an edit draws the
/// bubble at the size it had. No room is added for the caret: the field
/// paints it a point past its edge, inside the bubble's padding, and egui
/// wraps a line only once it is half a point wider than the field.
pub(super) fn inline_editor_width(
    text_width: f32,
    last_row: f32,
    drawn: f32,
    width: f32,
    reserve: Option<f32>,
) -> (f32, bool) {
    let inline = reserve.filter(|reserve| last_row + 8.0 + reserve <= width);
    let needed = match inline {
        Some(reserve) => text_width.max(last_row + 8.0 + reserve),
        None => text_width,
    };
    (needed.max(drawn).min(width), inline.is_some())
}

/// A small label over the bubble being edited, so it is clear that typing
/// goes into it and not into the composer. It floats over the transcript,
/// so the bubble keeps its place and size.
pub(super) fn editing_label(
    ui: &egui::Ui,
    view: &View<'_>,
    message: &Message,
    editor: egui::Id,
    at: Rect,
) {
    let ctx = ui.ctx();
    let id = editor.with("label");
    let shown = theme::animate(ctx, id, true, theme::MOTION_QUICK);
    // Where the bubble was last drawn, moved with the editor in case the
    // row has moved since.
    let bubble = ctx
        .data(|data| data.get_temp::<Rect>(bubble_id(&view.chat.id, &message.id).with("rect")))
        .zip(ctx.data(|data| data.get_temp::<Rect>(editor.with("rect"))))
        .map_or(at, |(bubble, before)| bubble.translate(at.min - before.min));
    ctx.data_mut(|data| data.insert_temp(editor.with("rect"), at));
    let (anchor, pivot) = if message.from_me {
        (bubble.right_top(), egui::Align2::RIGHT_BOTTOM)
    } else {
        (bubble.left_top(), egui::Align2::LEFT_BOTTOM)
    };
    let label = crate::i18n::gettext(view.locale, "Editing message");
    let palette = view.palette;
    egui::Area::new(id)
        .order(egui::Order::Tooltip)
        .interactable(false)
        .fixed_pos(anchor + vec2(0.0, -6.0 + 4.0 * (1.0 - shown)))
        .pivot(pivot)
        .constrain_to(ui.clip_rect())
        .show(ctx, |ui| {
            ui.multiply_opacity(shown);
            widgets::menu_frame(&palette)
                .inner_margin(Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (icon, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        theme::paint_icon(ui, Icon::Pencil, icon, 12.0, palette.accent);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(label.as_ref())
                                    .font(theme::semibold(theme::TEXT_CAPTION))
                                    .color(palette.accent),
                            )
                            .selectable(false)
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                });
        });
}

/// How long the rest of the chat takes to recede behind a message edited in
/// its bubble, and to come back once the edit ends: a panel's time, and
/// quicker on the way back, as a surface closes.
pub(super) const EDIT_FOCUS_IN: f32 = theme::MOTION_PANEL;
pub(super) const EDIT_FOCUS_OUT: f32 = theme::MOTION_PANEL * theme::CLOSE;

/// The layers the chat recedes on while a message is edited in its bubble:
/// the scrim over the whole conversation, and the edited row, kept directly
/// above the scrim and below menus opened after it.
pub(super) fn edit_focus_layers(chat: &str) -> (egui::LayerId, egui::LayerId) {
    let id = egui::Id::new(("edit-focus", chat));
    (
        egui::LayerId::new(egui::Order::Foreground, id),
        egui::LayerId::new(egui::Order::Foreground, id.with("row")),
    )
}

/// Where the open chat's transcript was laid out this frame, top to bottom.
/// Own balloons shade down it, and a lifted one, clipped to less, still does.
pub(super) fn transcript_span_id(chat: &str) -> egui::Id {
    in_window(egui::Id::new(("transcript-span", chat)))
}

/// The message edited in its bubble, and how far the rest of the chat has
/// receded behind it, from 0 to 1. The chat recedes while the editor has
/// the keyboard, or is about to take it, and comes back once the edit ends,
/// the bubble staying lifted until it has. Asked every frame the chat
/// shows, so the first edit fades in as later ones do; at once with a
/// screen reader.
pub(super) fn edit_focus(ctx: &egui::Context, chat: &str) -> Option<(String, f32)> {
    let slot = inline_edit_slot(chat);
    let editing = ctx
        .data(|data| data.get_temp::<(String, String)>(slot))
        .map(|(id, _)| id)
        .filter(|id| {
            let editor = bubble_id(chat, id).with("inline-editor");
            ctx.memory(|memory| memory.has_focus(editor))
                || !ctx.data(|data| data.get_temp::<bool>(editor.with("seen")).unwrap_or(false))
        });
    let kept = slot.with("focus");
    let seconds = if editing.is_some() {
        EDIT_FOCUS_IN
    } else {
        EDIT_FOCUS_OUT
    };
    let shown = theme::animate(ctx, kept.with("shown"), editing.is_some(), seconds);
    match editing {
        Some(id) => {
            ctx.data_mut(|data| data.insert_temp(kept, id.clone()));
            Some((id, shown))
        }
        None if shown > 0.0 => ctx
            .data(|data| data.get_temp::<String>(kept))
            .map(|id| (id, shown)),
        None => {
            ctx.data_mut(|data| data.remove::<String>(kept));
            None
        }
    }
}

/// The scrim's colour: the palette's shadow at 22% in a light theme and 40%
/// in a dark one, where the bubble needs more to stand out.
pub(super) fn edit_focus_shade(palette: &Palette) -> Color32 {
    let [r, g, b, _] = palette.shadow.to_srgba_unmultiplied();
    let weight: f32 = if palette.dark { 0.40 } else { 0.22 };
    Color32::from_rgba_unmultiplied(r, g, b, (weight * 255.0).round() as u8)
}

/// Recedes the chat behind a message edited in its bubble: a scrim over the
/// whole conversation, header and composer included, over a frost of it
/// where the window can frost (never on macOS, whose single Metal pass
/// cannot read back what it drew). The edited row is drawn on the layer
/// just above it, and the "Editing message" label above that. The scrim
/// holds no widgets, so clicks and scrolling reach what lies under it as
/// they did.
pub(super) fn paint_edit_focus(
    ctx: &egui::Context,
    palette: &Palette,
    chat: &str,
    card: Rect,
    radius: f32,
) {
    let Some((_, shown)) = edit_focus(ctx, chat) else {
        return;
    };
    let (scrim, row) = edit_focus_layers(chat);
    ctx.set_sublayer(scrim, row);
    let painter = ctx.layer_painter(scrim).with_clip_rect(card);
    painter.add(crate::ui::glass::veil(card, radius, shown));
    painter.rect_filled(
        card,
        radius,
        edit_focus_shade(palette).gamma_multiply(shown),
    );
}

/// Draws the editor in place of the message's text when it is being edited.
/// Enter saves, Shift+Enter breaks the line, Escape or a click elsewhere
/// puts the old text back. The bubble keeps the size it had and grows only
/// as the text outgrows it, with the time beside the last line where it
/// was. Returns `None` when this message is not being edited, else the
/// footer's place on the last line, as [`rich_body`] does.
pub(super) fn inline_editor(
    ui: &mut egui::Ui,
    view: &View<'_>,
    message: &Message,
    width: f32,
    reserve: Option<f32>,
    actions: &mut Vec<Action>,
) -> Option<Option<Rect>> {
    let slot = inline_edit_slot(&view.chat.id);
    let (id, mut text) = ui
        .ctx()
        .data(|data| data.get_temp::<(String, String)>(slot))?;
    if id != message.id {
        return None;
    }
    let drawn = ui
        .ctx()
        .data(|data| data.get_temp::<Vec2>(slot.with("size")))
        .unwrap_or(Vec2::ZERO);
    let palette = view.palette;
    let editor = bubble_id(&view.chat.id, &message.id).with("inline-editor");
    let focused = ui.memory(|memory| memory.has_focus(editor));
    let (save, cancel) = if focused {
        ui.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::Enter),
                input.consume_key(Modifiers::NONE, Key::Escape),
            )
        })
    } else {
        (false, false)
    };
    // Measured as it would wrap in the widest bubble, then drawn no wider
    // than that needs, so the bubble does not jump to its widest. The field
    // lays its text out the same way, so what was measured is what it draws.
    let font = theme::regular(BODY_SIZE);
    let lay_out = |ui: &egui::Ui, text: &str, wrap: f32| {
        ui.fonts_mut(|fonts| fonts.layout(text.to_owned(), font.clone(), palette.text, wrap))
    };
    let measured = lay_out(ui, &text, width);
    let last_row = measured.rows.last().map_or(0.0, |row| row.row.size.x);
    // The time stays beside the last line where the body put it there:
    // right-to-left text keeps it beside a single line only, and text under
    // a quote or a preview spans the card, as in `rich_body`.
    let card = message.quoted.is_some()
        || matches!(
            &message.content,
            Content::Text {
                preview: Some(_),
                ..
            }
        );
    let single = measured.rows.len() == 1 && !card;
    let reserve = reserve.filter(|_| !crate::bidi::message_rtl(&text) || single);
    let (fitted, inline) =
        inline_editor_width(measured.size().x, last_row, drawn.x, width, reserve);
    let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
        lay_out(ui, buffer.as_str(), wrap)
    };
    let output = egui::TextEdit::multiline(&mut text)
        .id(editor)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .font(theme::regular(BODY_SIZE))
        .text_color(palette.text)
        .desired_rows(1)
        .desired_width(fitted)
        // No shorter than the body was either, such as a big emoji-only
        // line, until the text needs more.
        .min_size(vec2(0.0, drawn.y))
        .layouter(&mut layouter)
        .show(ui);
    editing_label(ui, view, message, editor, output.response.rect);
    let first = !ui
        .ctx()
        .data(|data| data.get_temp::<bool>(editor.with("seen")).unwrap_or(false));
    if first {
        // Start with the caret after the text, and keep the field up to date
        // with the focus from here on.
        output.response.request_focus();
        let mut state = output.state.clone();
        let end = egui::text::CCursor::new(text.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ui.ctx(), editor);
        ui.ctx()
            .data_mut(|data| data.insert_temp(editor.with("seen"), true));
    }
    ui.ctx()
        .data_mut(|data| data.insert_temp(slot, (id.clone(), text.clone())));
    let finish = |ctx: &egui::Context| {
        ctx.data_mut(|data| {
            data.remove::<(String, String)>(slot);
            data.remove::<Vec2>(slot.with("size"));
            data.remove::<bool>(editor.with("seen"));
            data.remove::<Rect>(editor.with("rect"));
        });
    };
    let lost = output.response.lost_focus() && !save && !cancel;
    if save {
        let saved = text.trim().to_owned();
        let unchanged = matches!(&message.content, Content::Text { text, .. } if *text == saved);
        if !saved.is_empty() && !unchanged {
            actions.push(Action::SaveEdit {
                chat: view.chat.id.clone(),
                id: message.id.clone(),
                text: saved,
            });
        }
        finish(ui.ctx());
    } else if cancel || lost {
        finish(ui.ctx());
    }
    let rect = output.response.rect;
    Some(inline.then(|| {
        Rect::from_min_max(
            pos2(rect.right() - reserve.unwrap_or(0.0), rect.bottom() - 15.0),
            rect.right_bottom(),
        )
    }))
}

/// Where a video was drawn in its bubble, the rect the viewer flies from and
/// back to.
pub fn video_flight_id(chat: &str, message: &str) -> egui::Id {
    bubble_id(chat, message).with("video-flight")
}

/// Empties the message's picture while the viewer flies it, so it is not seen
/// twice: everything painted until [`show_source`] is drawn clear. Returns the
/// opacity to give back.
pub(super) fn hide_source(ui: &mut egui::Ui, message: &Message) -> f32 {
    let before = ui.opacity();
    if crate::ui::image_preview::source_hidden(ui.ctx(), &message.chat, &message.id) {
        ui.set_opacity(0.0);
    }
    before
}

/// Gives back what [`hide_source`] took.
pub(super) fn show_source(ui: &mut egui::Ui, opacity: f32) {
    ui.set_opacity(opacity);
}

/// Lets go of what a message's row left in `ctx.data` once the row has left the
/// layout. egui never frees temporary data, and every message that was ever on
/// screen would keep a dozen entries for the whole session. These are only
/// where the row's parts were last drawn, and the row writes them again
/// whenever it is laid out. Left alone: the arrival spring's start (without it
/// an old message would spring in again), the time toggle (the reader's own
/// choice), and the menu and reaction rects (a popup may still be anchored
/// to them).
pub(super) fn forget_bubble(ctx: &egui::Context, chat: &str, message: &str) {
    let id = bubble_id(chat, message);
    ctx.data_mut(|data| {
        for part in [
            "rect",
            "footer",
            "picture",
            "preview",
            "quote",
            "card",
            "call-box",
            "album-tile",
            "video-flight",
            "forwarded",
            "carousel-viewport",
        ] {
            data.remove::<Rect>(id.with(part));
        }
        data.remove::<bool>(id.with("swiping"));
        data.remove::<bool>(id.with("video-flight").with("note"));
    });
}

/// A Nothing bubble's room around its text.
pub(super) const NOTHING_TEXT_MARGIN: Margin = Margin::symmetric(14, 10);

/// Room between a call bubble's edge and the chip that holds its glyph
/// and words.
pub(super) const CALL_BUBBLE_PAD: i8 = 4;
