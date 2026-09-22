import { z } from 'zod'

/**
 * Validation shared by the form and the API boundary.
 *
 * These rules exist to catch typing mistakes before a round trip, not to
 * enforce security — the server validates everything again and is the only
 * thing that decides. Messages are written for the person filling in the form.
 */

export const loginSchema = z.object({
  username: z
    .string()
    .trim()
    .min(1, 'Enter your username.'),
  password: z
    .string()
    .min(1, 'Enter your password.'),
})

export type LoginInput = z.infer<typeof loginSchema>

export const changePasswordSchema = z
  .object({
    current_password: z.string().min(1, 'Enter your current password.'),
    new_password: z
      .string()
      .min(12, 'Use at least 12 characters. A short phrase you can remember is stronger than a short complicated word.'),
    confirm_password: z.string().min(1, 'Type your new password again.'),
  })
  .refine((v) => v.new_password === v.confirm_password, {
    path: ['confirm_password'],
    message: 'This does not match the new password above.',
  })
  .refine((v) => v.new_password !== v.current_password, {
    path: ['new_password'],
    message: 'Choose a password different from your current one.',
  })

export type ChangePasswordInput = z.infer<typeof changePasswordSchema>

export const userSummarySchema = z.object({
  id:                   z.string(),
  username:             z.string(),
  full_name:            z.string(),
  must_change_password: z.boolean(),
  permissions:          z.array(z.string()),
})

export type UserSummary = z.infer<typeof userSummarySchema>

export const sessionSchema = z.object({
  access_token:  z.string(),
  refresh_token: z.string(),
  expires_at:    z.string(),
  user:          userSummarySchema,
})

export type SessionResponse = z.infer<typeof sessionSchema>
