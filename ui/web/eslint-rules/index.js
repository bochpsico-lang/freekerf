// FreeKerf's own ESLint rules for ui/web (AGENTS.md, "UI rules").
//
// `freekerf/no-app-state-mutation` (the dispatch-only lint, PLAN §9 principle 7):
// app state lives in Rust and reaches the UI as read-only projections (machine,
// preview, view, settings...). Outside `src/state/` nothing may assign to them,
// update them or delete from them; changes go through `service.dispatch(action)`.
// Local, purely visual state (hover, a drag in progress) uses component `$state`.

/** Names under which the projections of app state are reachable. */
export const PROJECTIONS = ['machine', 'preview', 'view', 'settings', 'doc', 'job', 'console', 'materials'];

function chainNames(node) {
  const names = [];
  let n = node;
  while (n) {
    if (n.type === 'MemberExpression') {
      if (!n.computed && n.property.type === 'Identifier') names.push(n.property.name);
      n = n.object;
    } else if (n.type === 'ChainExpression') {
      n = n.expression;
    } else if (n.type === 'TSNonNullExpression') {
      n = n.expression;
    } else if (n.type === 'Identifier') {
      names.push(n.name);
      break;
    } else {
      break;
    }
  }
  return names;
}

const noAppStateMutation = {
  meta: {
    type: 'problem',
    docs: { description: 'App state changes only through dispatch(action) (PLAN §9).' },
    messages: {
      mutation:
        '`{{name}}` is a projection of app state: change it with service.dispatch(...), not by writing to it.',
    },
    schema: [],
  },
  create(context) {
    function check(target, node) {
      if (target.type !== 'MemberExpression') return;
      const names = chainNames(target);
      // the written property itself may be local (e.g. `canvas.style.x`); a projection
      // anywhere in the object chain is enough to report
      const hit = names.slice(1).find((n) => PROJECTIONS.includes(n));
      if (hit) context.report({ node, messageId: 'mutation', data: { name: hit } });
    }
    return {
      AssignmentExpression(node) {
        check(node.left, node);
      },
      UpdateExpression(node) {
        check(node.argument, node);
      },
      UnaryExpression(node) {
        if (node.operator === 'delete') check(node.argument, node);
      },
    };
  },
};

export default { rules: { 'no-app-state-mutation': noAppStateMutation } };
