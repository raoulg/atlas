// What a club keeps: its members, their visits, the reports they write and what hangs on its board.

export class Member {
  constructor(readonly name: string) {}
}

export class Visit {
  constructor(readonly member: string, readonly day: string) {}
}

export class Report {
  constructor(readonly title: string, readonly author: Member) {}
}

export class Post {
  constructor(readonly text: string) {}
}

export class Notice extends Post {
  constructor(text: string, readonly until: string) {
    super(text);
  }
}
