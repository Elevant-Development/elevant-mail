import { type ReactNode, useState } from "react";

import { Icon, type IconName } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { describeError } from "@/lib/errors";
import { displayName } from "@/lib/formatting";
import { locale, t } from "@/lib/i18n";
import { formatAttendees, formatEventTime, formatRecurrence } from "@/lib/invitations";
import type { Answer, Invitation } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { useCalendarSync } from "@/hooks/use-calendar-sync";
import { localDay } from "@/lib/calendar";
import { useDeleteEvent } from "@/queries/calendar";
import { useAnswerInvitation } from "@/queries/messages";
import { useCalendarStore } from "@/stores/calendar";

const answers: {
  answer: Answer;
  label: "invitation.accept" | "invitation.tentative" | "invitation.decline";
  icon: IconName;
}[] = [
  { answer: "accepted", label: "invitation.accept", icon: "check" },
  { answer: "tentative", label: "invitation.tentative", icon: "question" },
  { answer: "declined", label: "invitation.decline", icon: "close" },
];

function Row({ icon, label, children }: { icon: IconName; label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-2">
      <Icon name={icon} className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <span className="sr-only">{label}: </span>
      <span className="min-w-0 break-words">{children}</span>
    </div>
  );
}

/** Day and month on a small sheet, like a tear-off calendar. */
function DateSheet({ invitation }: { invitation: Invitation }) {
  const timeZone = invitation.allDay ? "UTC" : undefined;
  const month = new Intl.DateTimeFormat(locale, { month: "short", timeZone }).format(
    invitation.start,
  );
  const day = new Intl.DateTimeFormat(locale, { day: "numeric", timeZone }).format(
    invitation.start,
  );
  const cancelled = invitation.method === "cancel";
  return (
    <div
      aria-hidden
      className={cn(
        "flex w-12 shrink-0 flex-col items-center overflow-hidden rounded-lg border bg-card text-center",
        cancelled && "opacity-60",
      )}
    >
      <span
        className={cn(
          "w-full py-0.5 text-[10px] font-semibold tracking-wide uppercase",
          cancelled ? "bg-muted text-muted-foreground" : "bg-primary text-primary-foreground",
        )}
      >
        {month.replace(".", "")}
      </span>
      <span className="py-1 text-lg leading-none font-semibold tabular-nums">{day}</span>
    </div>
  );
}

interface InvitationCardProps {
  messageId: string;
  /** The mail's account: its calendars take an accepted event. */
  accountId: string;
  invitation: Invitation;
  /** The mail's subject, for an invitation without a title of its own. */
  subject: string | null;
}

/**
 * An invitation, cancellation or answer that came with a mail: when, where, who, and for an
 * invitation the buttons that answer the organizer by mail.
 */
export function InvitationCard({ messageId, accountId, invitation, subject }: InvitationCardProps) {
  const answer = useAnswerInvitation();
  const remove = useDeleteEvent();
  const { syncCalendars } = useCalendarSync();
  const showEvent = (eventId: string) => {
    const calendar = useCalendarStore.getState();
    calendar.setArea("calendar");
    calendar.setAnchor(invitation.allDay ? localDay(invitation.start) : invitation.start);
    calendar.openEvent(eventId);
  };
  const timeFormat = new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" });
  const [changing, setChanging] = useState(false);
  const title = invitation.title ?? subject ?? t("invitation.untitled");
  const cancelled = invitation.method === "cancel";
  const label = {
    request: t("invitation.label"),
    cancel: t("invitation.label.cancel"),
    reply: t("invitation.label.reply"),
    publish: t("invitation.label.publish"),
    other: t("invitation.label.publish"),
  }[invitation.method];
  const answered = invitation.answer;
  const asking =
    invitation.canAnswer && (answered === null || invitation.changedSinceAnswer || changing);
  const replier = invitation.method === "reply" ? invitation.attendees[0] : undefined;

  const send = (choice: Answer) => {
    answer.mutate(
      {
        messageId,
        answer: choice,
        subject: t(`invitation.mail.${choice}`, { title }),
        text: t(`invitation.mail.text.${choice}`, { title }),
      },
      {
        onSuccess: () => {
          setChanging(false);
          // The accepted event went into a calendar: off to its server.
          void syncCalendars(accountId);
        },
      },
    );
  };

  return (
    <section
      aria-label={`${label}: ${title}`}
      className="mt-5 rounded-xl border bg-muted/30 p-4 font-sans text-sm"
    >
      <div className="flex items-start gap-3">
        <DateSheet invitation={invitation} />
        <div className="min-w-0 flex-1">
          <p
            className={cn(
              "text-xs font-medium text-muted-foreground",
              cancelled && "text-destructive",
            )}
          >
            {label}
          </p>
          <h3
            className={cn(
              "text-base leading-snug font-semibold break-words",
              cancelled && "text-muted-foreground line-through",
            )}
          >
            {title}
          </h3>
          <div className="mt-2 grid gap-1.5">
            <Row icon="calendar" label={t("invitation.when")}>
              {formatEventTime(invitation, locale)}
            </Row>
            {invitation.recurrence && (
              <Row icon="repeat" label={t("invitation.repeats")}>
                {formatRecurrence(invitation.recurrence, locale)}
              </Row>
            )}
            {invitation.location && (
              <Row icon="mapPin" label={t("invitation.where")}>
                {invitation.location}
              </Row>
            )}
            {!replier && (invitation.organizer || invitation.attendees.length > 0) && (
              <Row icon="users" label={t("invitation.who")}>
                {formatAttendees(invitation, locale)}
              </Row>
            )}
            {invitation.conflicts.length > 0 && (
              <Row icon="alert" label={t("invitation.conflicts")}>
                <span className="text-amber-700 dark:text-amber-300">
                  {t("invitation.conflictsWith", {
                    events: new Intl.ListFormat(locale, { type: "conjunction" }).format(
                      invitation.conflicts.map(
                        (conflict) =>
                          `${conflict.title ?? t("invitation.untitled")} (${timeFormat.formatRange(conflict.start, conflict.end)})`,
                      ),
                    ),
                  })}
                </span>
              </Row>
            )}
            {invitation.inCalendar && !cancelled && (
              <Row icon="calendar" label={t("invitation.inCalendar")}>
                <button
                  type="button"
                  onClick={() => showEvent(invitation.inCalendar!.eventId)}
                  className="text-left underline-offset-2 hover:underline"
                >
                  {t("invitation.inCalendar")}
                </button>
              </Row>
            )}
          </div>
        </div>
      </div>

      {replier && (
        <p className="mt-3 flex items-center gap-2 border-t pt-3">
          <Icon
            name={
              replier.status === "accepted"
                ? "check"
                : replier.status === "declined"
                  ? "close"
                  : "question"
            }
            className={cn(
              "size-4 shrink-0",
              replier.status === "accepted" && "text-primary",
              replier.status === "declined" && "text-destructive",
            )}
          />
          <span>
            <span className="font-medium">{displayName(replier.address)}</span>{" "}
            {t(`invitation.status.${replier.status}`)}
          </span>
        </p>
      )}

      {cancelled && (
        <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-2 border-t pt-3 text-muted-foreground">
          <p className="flex items-center gap-2">
            <Icon name="calendarOff" className="size-4 shrink-0" />
            {t("invitation.cancelled")}
          </p>
          {invitation.inCalendar && (
            <Button
              variant="outline"
              size="sm"
              disabled={remove.isPending}
              onClick={() => {
                const entry = invitation.inCalendar!;
                remove.mutate({ eventId: entry.eventId, calendarId: entry.calendarId });
              }}
            >
              <Icon name="trash" />
              {t("invitation.removeFromCalendar")}
            </Button>
          )}
        </div>
      )}

      {invitation.canAnswer && (
        <div className="mt-3 grid gap-2 border-t pt-3">
          {answered && !asking && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
              <p className="flex items-center gap-2">
                <Icon
                  name={answers.find((a) => a.answer === answered)?.icon ?? "check"}
                  className="size-4 shrink-0 text-primary"
                />
                {t(`invitation.answered.${answered}`)}
              </p>
              <Button variant="ghost" size="sm" onClick={() => setChanging(true)}>
                {t("invitation.change")}
              </Button>
            </div>
          )}
          {asking && (
            <>
              {answered && invitation.changedSinceAnswer && (
                <p className="text-muted-foreground">{t("invitation.changed")}</p>
              )}
              <div
                role="group"
                aria-label={t("invitation.answer")}
                className="flex flex-wrap gap-2"
              >
                {answers.map(({ answer: choice, label: choiceLabel, icon }) => {
                  const sending = answer.isPending && answer.variables.answer === choice;
                  return (
                    <Button
                      key={choice}
                      // The answer given so far stands out; without one, accepting does.
                      variant={choice === (answered ?? "accepted") ? "default" : "outline"}
                      size="sm"
                      aria-pressed={answered === choice}
                      disabled={answer.isPending}
                      onClick={() => send(choice)}
                    >
                      <Icon
                        name={sending ? "spinner" : icon}
                        className={cn(sending && "animate-spin")}
                      />
                      {t(choiceLabel)}
                    </Button>
                  );
                })}
                {changing && !invitation.changedSinceAnswer && (
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={answer.isPending}
                    onClick={() => setChanging(false)}
                  >
                    {t("invitation.keep")}
                  </Button>
                )}
              </div>
              <p className="text-xs text-muted-foreground" aria-live="polite">
                {answer.isPending
                  ? t("invitation.sending")
                  : invitation.organizer &&
                    t("invitation.sentTo", { name: displayName(invitation.organizer) })}
              </p>
            </>
          )}
          {answer.isError && (
            <Notice
              tone="error"
              text={describeError(answer.error).message}
              detail={describeError(answer.error).action}
            />
          )}
        </div>
      )}
    </section>
  );
}
