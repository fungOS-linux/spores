# The tray

`notify` presents what wants attention and nothing more: a stack of
notifications with the app that raised them, a focus state, and history when the
user opted into it. Acting on a notification is a host action; the Spore names it.

# Focus

Focus profiles are per device. Setting one does not silence another machine, and
silencing a category here does not edit another app's settings — it records that
this device is not to be interrupted.

# Ringer

Calls, doorbells, alarms and pages arrive through the ringer grant. Answering is
an explicit action with a duration; a ringer that cannot reach the host says so
rather than pretending the call ended.

# What it cannot do

It does not publish notifications: that grant is deliberately absent, so a Spore
cannot make the tray speak for another app. Network access is a separate grant it
does not hold.
