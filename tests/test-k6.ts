import ws from "k6/ws";
import http from "k6/http";
import { check } from "k6";
import { Counter } from "k6/metrics";
import { randomString } from "https://jslib.k6.io/k6-utils/1.2.0/index.js";
import execution from "k6/execution";

const BASE_URL = "http://localhost:8000";
const WS_URL = "ws://localhost:8000";
const LISTENER_COUNT = 10;
const CHAT_NAME = "load_test_room";

const sentMessages = new Counter("sent_messages");
const receivedMessages = new Counter("received_messages");

export const options = {
  scenarios: {
    listener: {
      executor: "per-vu-iterations",
      vus: LISTENER_COUNT,
      iterations: 1,
      maxDuration: "1m",
      exec: "runListener",
    },
    spammers: {
      executor: "ramping-arrival-rate",
      startRate: 50,
      timeUnit: "1s",
      preAllocatedVUs: 50,
      maxVUs: 100,
      stages: [
        { target: 1000, duration: "30s" },
        { target: 1000, duration: "30s" },
        { target: 10000, duration: "30s" },
        { target: 10000, duration: "30s" },
        { target: 0, duration: "10s" },
      ],
      exec: "runSpammer",
    },
  },
};

export function setup() {
  const spammerUser = createUser(`spammer_${randomString(5)}`);

  const listenerUsers = [];
  for (let i = 0; i < LISTENER_COUNT; i++) {
    listenerUsers.push(createUser(`listener_${i}_${randomString(5)}`));
  }

  const allMemberIds = [
    spammerUser.userId,
    ...listenerUsers.map((u) => u.userId),
  ];

  const chatPayload = { name: CHAT_NAME, members: allMemberIds };

  let body = `name=${CHAT_NAME}`;
  allMemberIds.forEach((id) => {
    body += `&members=${id}`;
  });

  const res = http.post(`${BASE_URL}/chats`, body, {
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
  });

  if (res.status !== 201 && res.status !== 200) {
    console.error("Create chat failed. Status:", res.status, "Body:", res.body);
    throw new Error("Failed to create chat");
  }

  const chatId = res.json().chat_id;

  return {
    spammerUser,
    listenerUsers,
    chatId,
  };
}

function createUser(username) {
  let res = http.post(`${BASE_URL}/users`, { username });
  check(res, { "user created": (r) => r.status === 201 || r.status === 200 });

  res = http.post(`${BASE_URL}/login`, { username });
  const jar = http.cookieJar();
  const cookies = jar.cookiesForURL(BASE_URL);
  const userId = cookies.user_id[0];

  return { userId, username };
}

export function runListener(data) {
  const myIndex = execution.vu.idInInstance;
  const user = data.listenerUsers[myIndex % data.listenerUsers.length];

  const url = `${WS_URL}/ws/connect/${user.userId}`;

  const response = ws.connect(url, {}, function (socket) {
    socket.on("open", () => console.log(`Listener ${myIndex} Connected`));

    socket.on("message", (msg) => {
      receivedMessages.add(1);
    });

    socket.on("close", () => console.log(`Listener ${myIndex} Disconnected`));

    socket.setTimeout(() => {
      socket.close();
    }, 70000);
  });

  check(response, { "ws connected": (r) => r && r.status === 101 });
}

export function runSpammer(data) {
  const user = data.spammerUser;
  const payload = { content: `Stress test message ${Date.now()}` };

  const params = {
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Cookie: `user_id=${user.userId}`,
    },
  };

  const res = http.post(
    `${BASE_URL}/chats/${data.chatId}/messages`,
    payload,
    params,
  );

  if (check(res, { "msg sent": (r) => r.status === 200 })) {
    sentMessages.add(1);
  }
}
