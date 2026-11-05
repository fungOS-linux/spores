return {
  revision = 3,
  title = "Inbox",
  brief = "2 need replies · 1 deadline",
  sections = {
    {
      collection = "email.message",
      label = "Mail",
      records = {
        { id = "work/INBOX/42/7", title = "Quarterly plan", summary = "Avery asked for the deck by Friday", unread = true, urgent = true, selected = true },
        { id = "work/INBOX/42/8", title = "Invoice received", summary = "No action", unread = false },
      },
    },
    {
      collection = "chat.message",
      label = "Chat",
      records = {
        { id = "irc-fleet/$abc", title = "avery in #agents", summary = "deploy is green", unread = true },
      },
    },
  },
}
