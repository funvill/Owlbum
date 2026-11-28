# Notes for agents working on Owlbum

Read [docs/build-process.md](docs/build-process.md) first. It holds how the build is run and resumed, the owner's standing orders, the lessons learned and the agent brief. The specification is in [docs/](docs/README.md); decisions are recorded on the GitHub issues.

## Owner feedback on a version

When the owner reviews a version (a test build such as 0.1.4) and gives feedback:

1. **First, create one GitHub issue for the feedback**, titled `Feedback on <version>`. Put the owner's feedback in it as given, one checklist item per point.
2. **Research every item before starting work**, and add the findings to the feedback issue: the code and spec sections involved, the likely cause of each bug, the intended approach, and any conflict with the spec or with earlier feedback (say which wins: the owner's newest words).
3. **An item large enough to need its own issue gets one.** Link it from the feedback issue's checklist, and link back from the new issue to the feedback issue. Small items are done under the feedback issue itself.
4. **Close the feedback issue when all of its feedback is addressed.** Tick each checklist item as it lands, naming the commit or pull request. An item that cannot be done as asked is recorded on the issue with what was done instead.
