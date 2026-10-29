// The rules of a new password: on the create-file screen and when the password changes.

export const MIN_PASSWORD = 8;

/** What is wrong with a new password, or null when it can be used. */
export function passwordProblem(password: string, repeat: string, written: boolean): string | null {
  if (password.length < MIN_PASSWORD) {
    return `Пароль — не короче ${MIN_PASSWORD.toString()} символов.`;
  }
  if (password !== repeat) return "Пароли не совпадают.";
  if (!written) return "Отметьте, что пароль записан.";
  return null;
}
